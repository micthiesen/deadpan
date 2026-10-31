//! Actual flattened Root/Domain construction, shared with compatibility queries.
use super::*;
use crate::AudioSoundContextAdmission;
use crate::plan::audio_owners::admission::{self, Budget as QueryBudget, Memory, Owned, Result};

/// An immutable flattened query and its independent RAII charge stay inseparable.
/// No public constructor or detachment can certify an unadmitted query.
pub struct AdmittedAudioQuery<I, L> {
    query: Owned<AudioQuery, I, L>,
    canonical: usize,
    additional: usize,
    forwarded: bool,
}
impl<I, L> AdmittedAudioQuery<I, L> {
    pub fn query(&self) -> &AudioQuery {
        &self.query.value
    }
    pub fn work(&self) -> usize {
        self.query.value.work
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
impl AdmittedAudioQuery<(), ()> {
    pub(in crate::plan) fn unmetered(self) -> AudioQuery {
        self.query.unmetered()
    }
}

pub(in crate::plan) struct FlatBuffer<I, L> {
    pub(in crate::plan) query: AudioQuery,
    pub(in crate::plan) capacity: Memory<I, L>,
}
pub(in crate::plan) type FlatValue<A> = Owned<
    FlatBuffer<<A as AudioSoundContextAdmission>::Issuer, <A as AudioSoundContextAdmission>::Lease>,
    <A as AudioSoundContextAdmission>::Issuer,
    <A as AudioSoundContextAdmission>::Lease,
>;

// Every actual value precedes its payload and replaceable vector-buffer charge,
// including a partial header clone and all earlier spans on any failed walk.
struct Build<I, L> {
    project_id: Option<ProjectId>,
    revision_id: Option<RevisionId>,
    spans: Vec<AudioSpan>,
    payload: Memory<I, L>,
    capacity: Memory<I, L>,
}

impl RenderPlan {
    /// Bounded flattened lookup with actual work and allocation-site admission.
    /// The carrier keeps its charge live until the query's last borrow ends.
    pub fn audio_admitted<A: AudioSoundContextAdmission>(
        &self,
        samples: Range<AudioSample>,
        limits: AudioQueryLimits,
        admission: &mut A,
    ) -> std::result::Result<AdmittedAudioQuery<A::Issuer, A::Lease>, A::Error> {
        limits.validate()?;
        if samples.start.0 < 0
            || samples.end < samples.start
            || samples.end > self.audio_duration()?
        {
            return Err(PlanError::AudioRangeOutOfRange.into());
        }
        let local = Budget {
            remaining: limits.maximum_work,
            lookup: LookupStats::default(),
        };
        query(self, None, samples, limits, local, admission)
    }
}

pub(in crate::plan) fn query<A: AudioSoundContextAdmission>(
    plan: &RenderPlan,
    seed: Option<&AudioWalkSeed>,
    samples: Range<AudioSample>,
    limits: AudioQueryLimits,
    mut local: Budget,
    admission: &mut A,
) -> std::result::Result<AdmittedAudioQuery<A::Issuer, A::Lease>, A::Error> {
    let mut budget = QueryBudget::new(&mut local, admission);
    admission::outer::<_, A>((|| {
        let start = budget.checkpoint();
        let value = child_query(plan, seed, samples, limits, &mut budget)?;
        let usage = budget.usage_since(start)?;
        if limits.maximum_work.checked_sub(budget.remaining) != Some(usage.work)
            || value.query.work != usage.work
        {
            return Err(PlanError::InvalidPlan("flattened query work provenance mismatch").into());
        }
        let query = value.try_map(&mut budget, |mut value, memory, budget| {
            budget.check_memory(&value.capacity)?;
            budget.merge_checked(memory, &mut value.capacity);
            budget.check_memory(memory)?;
            Ok(value.query)
        })?;
        Ok(AdmittedAudioQuery {
            query,
            canonical: usage.canonical,
            additional: usage.additional,
            forwarded: A::METERED,
        })
    })())
}

/// The caller preserves the actual Root/Domain range validation before assembly.
pub(in crate::plan) fn child_query<A: AudioSoundContextAdmission>(
    plan: &RenderPlan,
    seed: Option<&AudioWalkSeed>,
    samples: Range<AudioSample>,
    limits: AudioQueryLimits,
    budget: &mut QueryBudget<'_, A>,
) -> Result<FlatValue<A>, A> {
    let start = budget.checkpoint();
    if limits.maximum_work != start.remaining() {
        return Err(PlanError::InvalidPlan("flattened query work provenance mismatch").into());
    }
    let mut build = Build {
        project_id: None,
        revision_id: None,
        spans: Vec::new(),
        payload: budget.memory(),
        capacity: budget.memory(),
    };
    let mut cursor = samples.start;
    while cursor < samples.end {
        if build.spans.len() == limits.maximum_spans {
            return Err(PlanError::AudioQueryLimit("span count").into());
        }
        let mut span = plan
            .audio_walk_inner_admitted(
                cursor,
                budget,
                seed,
                false,
                false,
                (None, None, None, None),
            )?
            .flattened_leaf(budget)?;
        span.value.samples = cursor..span.value.allocated_samples.end.min(samples.end);
        cursor = span.value.samples.end;
        let needed = build
            .spans
            .len()
            .checked_add(1)
            .ok_or(TimeError::Overflow)?;
        // The whole replacement, the old buffer, earlier contents and this
        // current span remain charged together while grow allocates and moves.
        budget.grow(&mut build.spans, &mut build.capacity, needed)?;
        budget.check_memory(&build.payload)?;
        budget.check_memory(&span.memory)?;
        build.spans.push(span.value);
        budget.merge_checked(&mut build.payload, &mut span.memory);
    }
    // Preserve the legacy header-clone point after every span is constructed.
    build.project_id = Some(budget.project(&mut build.payload, &plan.metadata.project_id)?);
    build.revision_id = Some(budget.revision(&mut build.payload, &plan.metadata.revision_id)?);
    budget.check_memory(&build.payload)?;
    budget.check_memory(&build.capacity)?;
    let usage = budget.usage_since(start)?;
    let work = usage.work;
    if limits.maximum_work.checked_sub(budget.remaining) != Some(work) {
        return Err(PlanError::InvalidPlan("flattened query work provenance mismatch").into());
    }
    let query = Owned {
        value: FlatBuffer {
            query: AudioQuery {
                project_id: build
                    .project_id
                    .expect("completed flattened project header"),
                revision_id: build
                    .revision_id
                    .expect("completed flattened revision header"),
                samples,
                spans: build.spans,
                lookup: usage.lookup,
                work,
            },
            capacity: build.capacity,
        },
        memory: build.payload,
    };
    budget.check_memory(&query.memory)?;
    Ok(query)
}
