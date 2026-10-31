//! Actual standalone retained creative fades; history keeps its original anchor.
use super::*;
use crate::AdmittedAudioFadeQuery;
use crate::plan::audio::capture::storage;
use crate::plan::audio::processing_query as collection;
use crate::plan::audio_bound::query::GuardedBound;
use crate::plan::audio_fades::query as fades;
use crate::plan::audio_owners::admission::{Memory, outer};

#[cfg(test)]
#[path = "audio_retained_fades_query_trace.rs"]
pub(in crate::plan) mod trace;

type CopiedProcessing<'plan, A> = GuardedBound<
    AudioWalkSpan<'plan>,
    <A as AudioSoundContextAdmission>::Issuer,
    <A as AudioSoundContextAdmission>::Lease,
>;
type CopiedEditorial<A> = Owned<
    EditorialCapture,
    <A as AudioSoundContextAdmission>::Issuer,
    <A as AudioSoundContextAdmission>::Lease,
>;

struct ProcessingFields {
    definition: Option<super::super::AudioDefinitionSelector>,
    instance: Option<deadpan_core::InstancePath>,
    gap_after: Option<deadpan_core::IterationId>,
    retimes: Vec<super::super::AudioRetimeStage>,
}

fn processing_copy<'plan, A: AudioSoundContextAdmission>(
    value: &AudioProcessingSpan<'plan>,
    budget: &mut QueryBudget<'_, A>,
) -> QueryResult<CopiedProcessing<'plan, A>, A> {
    let AudioProcessingSpan {
        definition,
        samples,
        allocated_samples,
        project_extent,
        instance,
        gap_after,
        transform,
        grid,
        sampling,
        retimes,
        content,
    } = value;
    budget.additional(1)?;
    let mut fields = Owned {
        value: ProcessingFields {
            definition: None,
            instance: None,
            gap_after: None,
            retimes: Vec::new(),
        },
        memory: budget.memory(),
    };
    fields.value.definition = budget.definition(&mut fields.memory, definition)?;
    fields.value.instance = Some(budget.instance_fixed(&mut fields.memory, instance)?);
    fields.value.gap_after = gap_after
        .as_ref()
        .map(|value| budget.iteration(&mut fields.memory, value))
        .transpose()?;
    fields.value.retimes = storage::fixed(retimes, &mut fields.memory, budget, storage::retime)?;
    let AudioSignalContent::Bound(bound) = content else {
        return Err(PlanError::InvalidPlan("fade has no bound output").into());
    };
    let copy = bound.copy_for_retained_fade_query(budget)?;
    let copy = GuardedBound::plain(copy.value, budget.issuer).adopt_memory(copy.memory, budget)?;
    #[cfg(test)]
    trace::observe(trace::Phase::BoundBoxBefore);
    let content = copy.boxed(budget)?;
    #[cfg(test)]
    trace::observe(trace::Phase::BoundBoxAfter);
    struct Mapping<C, F, I, L> {
        content: C,
        fields: F,
        memory: Memory<I, L>,
    }
    let pending = Mapping {
        content,
        fields: fields.value,
        memory: fields.memory,
    };
    budget.check_memory(&pending.memory)?;
    let value = pending.content.map(budget, |content| {
        let ProcessingFields {
            definition,
            instance,
            gap_after,
            retimes,
        } = pending.fields;
        AudioWalkSpan::Stage(AudioProcessingSpan {
            definition,
            samples: samples.clone(),
            allocated_samples: allocated_samples.clone(),
            project_extent: project_extent.clone(),
            instance: instance.expect("copied processing instance"),
            gap_after,
            transform: *transform,
            grid: *grid,
            sampling: *sampling,
            retimes,
            content: AudioSignalContent::Bound(content),
        })
    })?;
    value.adopt_memory(pending.memory, budget)
}

fn editorial_copy<A: AudioSoundContextAdmission>(
    value: &EditorialCapture,
    budget: &mut QueryBudget<'_, A>,
) -> QueryResult<CopiedEditorial<A>, A> {
    budget.additional(1)?;
    let EditorialCapture {
        editorial,
        constraints,
        repeats,
    } = value;
    let mut copy = Owned {
        value: EditorialCapture::default(),
        memory: budget.memory(),
    };
    copy.value.editorial = storage::fixed(editorial, &mut copy.memory, budget, storage::editorial)?;
    copy.value.constraints = storage::fixed(
        constraints,
        &mut copy.memory,
        budget,
        |value, memory, budget| budget.constraint(memory, value),
    )?;
    copy.value.repeats = storage::fixed(
        repeats,
        &mut copy.memory,
        budget,
        |value, memory, budget| budget.repeat(memory, value),
    )?;
    budget.check_memory(&copy.memory)?;
    Ok(copy)
}

impl AudioOwnerRetainedInput<'_> {
    /// Creative fades on the original consumer grid, admitting the actual
    /// copies and history inspection while preserving the stored binding work.
    pub fn fades_admitted<A: AudioSoundContextAdmission>(
        &self,
        samples: std::ops::Range<AudioSample>,
        limits: AudioQueryLimits,
        admission: &mut A,
    ) -> Result<AdmittedAudioFadeQuery<A::Issuer, A::Lease>, A::Error> {
        if !A::METERED {
            let query = self.fades(samples, limits).map_err(A::Error::from)?;
            return outer::<_, A>(fades::ordinary(query, admission));
        }
        limits.validate()?;
        if samples.end < samples.start {
            return Err(PlanError::AudioRangeOutOfRange.into());
        }
        let mut local = Budget {
            remaining: limits.maximum_work,
            lookup: LookupStats::default(),
        };
        let mut budget = QueryBudget::new(&mut local, admission);
        outer::<_, A>((|| {
            let mut spans = collection::empty(&budget);
            let mut cursor = samples.start;
            while cursor < samples.end {
                if spans.spans.len() == limits.maximum_spans {
                    return Err(PlanError::AudioQueryLimit("retained fade span count").into());
                }
                budget.canonical(
                    1 + self.bound().work()
                        + self.processing.retimes.len()
                        + self.processing.instance.repeats.len()
                        + self.editorial.editorial.len()
                        + self.editorial.constraints.len()
                        + self.editorial.repeats.len(),
                )?;
                #[cfg(test)]
                trace::observe(trace::Phase::ProcessingCopyBefore);
                let primary = processing_copy(&self.processing, &mut budget)?;
                #[cfg(test)]
                trace::observe(trace::Phase::ProcessingCopyAfter);
                #[cfg(test)]
                trace::observe(trace::Phase::EditorialCopyBefore);
                let editorial = editorial_copy(&self.editorial, &mut budget)?;
                #[cfg(test)]
                trace::observe(trace::Phase::EditorialCopyAfter);
                let span = fades::retained_bound_span_admitted(
                    primary,
                    cursor,
                    samples.end,
                    editorial,
                    &mut budget,
                )?;
                if span.samples.start != cursor
                    || span.samples.end <= cursor
                    || span.samples.end > samples.end
                {
                    return Err(
                        PlanError::InvalidPlan("retained fade interval did not advance").into(),
                    );
                }
                cursor = span.samples.end;
                #[cfg(test)]
                trace::observe(trace::Phase::OuterPushBefore {
                    old_len: spans.spans.len(),
                    old_capacity: spans.spans.capacity(),
                });
                spans = collection::push(spans, span, &mut budget)?;
                #[cfg(test)]
                trace::observe(trace::Phase::OuterPushAfter {
                    new_len: spans.spans.len(),
                    new_capacity: spans.spans.capacity(),
                });
            }
            fades::finish_query(spans, limits.maximum_work, &mut budget)
        })())
    }
}
