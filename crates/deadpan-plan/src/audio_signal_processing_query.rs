//! Actual Root signal assembly and processing conversion.
use super::*;
use crate::audio_allocated::admission::array;
use crate::plan::audio::processing_query as collection;
use crate::plan::audio_bound::query::GuardedBound;
use crate::plan::audio_owners::admission::Memory;

struct SignalBuffer<'plan, I, L> {
    query: AudioSignalQuery<'plan>,
    capacity: Memory<I, L>,
}

type SignalValue<'plan, A> = GuardedBound<
    SignalBuffer<
        'plan,
        <A as AudioSoundContextAdmission>::Issuer,
        <A as AudioSoundContextAdmission>::Lease,
    >,
    <A as AudioSoundContextAdmission>::Issuer,
    <A as AudioSoundContextAdmission>::Lease,
>;
type SignalResult<'plan, A> = QueryResult<SignalValue<'plan, A>, A>;

impl<'plan> AudioSignal<'plan> {
    fn structural_grid(
        &self,
        samples: &Range<SignalSample>,
        limits: AudioQueryLimits,
        rule: AudioBoundaryRule,
    ) -> Result<AudioSampleGrid<SignalSample>, PlanError> {
        limits.validate()?;
        if matches!(self.provider, SignalProvider::RequiresOwnedInput) {
            return Err(PlanError::InvalidPlan(
                "owned Preserve input requires its compiled projection",
            ));
        }
        let grid = self.transform()?.grid(rule)?;
        let first = grid.boundary(self.support.start)?;
        let count = grid.boundary(self.support.end)?;
        if samples.start < first || samples.end < samples.start || samples.end > count {
            return Err(PlanError::AudioRangeOutOfRange);
        }
        Ok(grid)
    }

    pub(super) fn structural_query_unmetered(
        &self,
        samples: Range<SignalSample>,
        limits: AudioQueryLimits,
        rule: AudioBoundaryRule,
        stop_at_preserve: bool,
        stop_at_bindings: bool,
    ) -> Result<AudioSignalQuery<'plan>, PlanError> {
        let grid = self.structural_grid(&samples, limits, rule)?;
        let mut local = Budget {
            remaining: limits.maximum_work,
            lookup: LookupStats::default(),
        };
        let mut admission = Unmetered;
        outer::<_, Unmetered>(self.structural_query_admitted(
            samples,
            limits,
            grid,
            stop_at_preserve,
            stop_at_bindings,
            &mut QueryBudget::new(&mut local, &mut admission),
        ))
        .map(|value| value.unmetered().query)
    }

    fn structural_query_admitted<A: AudioSoundContextAdmission>(
        &self,
        samples: Range<SignalSample>,
        limits: AudioQueryLimits,
        grid: AudioSampleGrid<SignalSample>,
        stop_at_preserve: bool,
        stop_at_bindings: bool,
        budget: &mut QueryBudget<'_, A>,
    ) -> SignalResult<'plan, A> {
        let mut accumulator = collection::empty(budget);
        let mut cursor = samples.start;
        while cursor < samples.end {
            if accumulator.spans.len() == limits.maximum_spans {
                return Err(PlanError::AudioQueryLimit("span count").into());
            }
            let span =
                self.span_admitted(cursor, grid, stop_at_preserve, stop_at_bindings, budget)?;
            let end = span.allocated_samples.end.min(samples.end);
            let span = span.map(budget, |mut span| {
                span.samples = cursor..end;
                span
            })?;
            cursor = end;
            accumulator = collection::push(accumulator, span, budget)?;
        }
        let mut header = Owned {
            value: (None, None, None),
            memory: budget.memory(),
        };
        header.value.0 = Some(budget.project(&mut header.memory, &self.plan.metadata.project_id)?);
        header.value.1 =
            Some(budget.revision(&mut header.memory, &self.plan.metadata.revision_id)?);
        header.value.2 = budget.definition(&mut header.memory, &self.definition)?;
        let header =
            GuardedBound::plain(header.value, budget.issuer).adopt_memory(header.memory, budget)?;
        let work = budget
            .canonical_used
            .checked_add(budget.additional_used)
            .ok_or(TimeError::Overflow)?;
        if limits.maximum_work.checked_sub(budget.remaining) != Some(work) {
            return Err(PlanError::InvalidPlan("processing query work provenance mismatch").into());
        }
        let lookup = budget.lookup;
        accumulator
            .zip(header, budget)?
            .map(budget, |(accumulator, header)| SignalBuffer {
                query: AudioSignalQuery {
                    project_id: header.0.expect("completed signal project header"),
                    revision_id: header.1.expect("completed signal revision header"),
                    definition: header.2,
                    samples,
                    spans: accumulator.spans,
                    lookup,
                    work,
                },
                capacity: accumulator.capacity,
            })
    }
}

impl RenderPlan {
    /// Retain the actual Root processing query and its independent construction leases.
    pub fn audio_processing_admitted<A: AudioSoundContextAdmission>(
        &self,
        samples: Range<AudioSample>,
        limits: AudioQueryLimits,
        admission: &mut A,
    ) -> Result<crate::AdmittedAudioProcessingQuery<'_, A::Issuer, A::Lease>, A::Error> {
        if !A::METERED {
            let signal = self
                .audio_signal()
                .query_inner(
                    SignalSample(samples.start.0)..SignalSample(samples.end.0),
                    limits,
                    AudioBoundaryRule::RoundEven,
                    true,
                    true,
                )
                .map_err(A::Error::from)?;
            let query = ordinary_conversion(signal, samples).map_err(A::Error::from)?;
            return outer::<_, A>(collection::ordinary(query, admission));
        }
        let signal = self.audio_signal();
        let signal_samples = SignalSample(samples.start.0)..SignalSample(samples.end.0);
        let grid = signal
            .structural_grid(&signal_samples, limits, AudioBoundaryRule::RoundEven)
            .map_err(A::Error::from)?;
        let mut local = Budget {
            remaining: limits.maximum_work,
            lookup: LookupStats::default(),
        };
        let mut budget = QueryBudget::new(&mut local, admission);
        outer::<_, A>((|| {
            let input = signal.structural_query_admitted(
                signal_samples,
                limits,
                grid,
                true,
                true,
                &mut budget,
            )?;
            convert(input, samples, limits, &mut budget)
        })())
    }
}

struct Conversion<'plan, I, L> {
    project_id: ProjectId,
    revision_id: RevisionId,
    definition: Option<AudioDefinitionSelector>,
    old: std::vec::IntoIter<AudioSignalSpan<'plan>>,
    output: Vec<AudioProcessingSpan<'plan>>,
    input_capacity: Memory<I, L>,
    output_capacity: Memory<I, L>,
}

fn convert<'plan, A: AudioSoundContextAdmission>(
    input: SignalValue<'plan, A>,
    samples: Range<AudioSample>,
    limits: AudioQueryLimits,
    budget: &mut QueryBudget<'_, A>,
) -> QueryResult<crate::AdmittedAudioProcessingQuery<'plan, A::Issuer, A::Lease>, A> {
    let count = input.query.spans.len();
    let mut output = Owned {
        value: Vec::new(),
        memory: budget.memory(),
    };
    budget.reserve(
        &mut output.memory,
        array::<AudioProcessingSpan<'plan>>(count)?,
    )?;
    output.value = Vec::with_capacity(count);
    assert_eq!(output.value.capacity(), count);
    let mut state = input.map(budget, move |input| Conversion {
        project_id: input.query.project_id,
        revision_id: input.query.revision_id,
        definition: input.query.definition,
        old: input.query.spans.into_iter(),
        output: output.value,
        input_capacity: input.capacity,
        output_capacity: output.memory,
    })?;
    while let Some(next) = state.old.as_slice().first() {
        let grid = AudioSampleGrid::new(
            next.grid.frame_origin(),
            next.grid.frames_per_sample(),
            next.grid.boundary_rule(),
        )?;
        let sampling = AudioSampleMap::new(
            AudioSample(next.sampling.anchor().0),
            next.sampling.local_at_anchor(),
            next.sampling.local_frames_per_sample(),
        )?;
        budget.additional(1)?;
        state = state.map(budget, |mut state| {
            let span = state.old.next().expect("observed signal conversion span");
            state.output.push(AudioProcessingSpan {
                definition: span.definition,
                samples: AudioSample(span.samples.start.0)..AudioSample(span.samples.end.0),
                allocated_samples: AudioSample(span.allocated_samples.start.0)
                    ..AudioSample(span.allocated_samples.end.0),
                project_extent: span.signal_extent,
                instance: span.instance,
                gap_after: span.gap_after,
                transform: AudioTransform {
                    project_origin: span.transform.signal_origin,
                    project_frames_per_local_frame: span.transform.signal_frames_per_local_frame,
                    project_frames_per_sample: span.transform.signal_frames_per_sample,
                },
                grid,
                sampling,
                retimes: span.retimes,
                content: span.content,
            });
            state
        })?;
    }
    let work = budget
        .canonical_used
        .checked_add(budget.additional_used)
        .ok_or(TimeError::Overflow)?;
    let lookup = budget.lookup;
    let query = state.map(budget, |state| {
        let Conversion {
            project_id,
            revision_id,
            definition,
            old,
            output,
            input_capacity,
            output_capacity,
        } = state;
        drop(definition);
        drop(old);
        drop(input_capacity);
        (
            AudioProcessingQuery {
                project_id,
                revision_id,
                samples,
                spans: output,
                lookup,
                work,
            },
            output_capacity,
        )
    })?;
    collection::seal(query, limits, budget)
}

fn ordinary_conversion<'plan>(
    query: AudioSignalQuery<'plan>,
    samples: Range<AudioSample>,
) -> Result<AudioProcessingQuery<'plan>, PlanError> {
    Ok(AudioProcessingQuery {
        project_id: query.project_id,
        revision_id: query.revision_id,
        samples,
        spans: query
            .spans
            .into_iter()
            .map(|span| {
                Ok(AudioProcessingSpan {
                    definition: span.definition,
                    samples: AudioSample(span.samples.start.0)..AudioSample(span.samples.end.0),
                    allocated_samples: AudioSample(span.allocated_samples.start.0)
                        ..AudioSample(span.allocated_samples.end.0),
                    project_extent: span.signal_extent,
                    instance: span.instance,
                    gap_after: span.gap_after,
                    transform: AudioTransform {
                        project_origin: span.transform.signal_origin,
                        project_frames_per_local_frame: span
                            .transform
                            .signal_frames_per_local_frame,
                        project_frames_per_sample: span.transform.signal_frames_per_sample,
                    },
                    grid: AudioSampleGrid::new(
                        span.grid.frame_origin(),
                        span.grid.frames_per_sample(),
                        span.grid.boundary_rule(),
                    )?,
                    sampling: AudioSampleMap::new(
                        AudioSample(span.sampling.anchor().0),
                        span.sampling.local_at_anchor(),
                        span.sampling.local_frames_per_sample(),
                    )?,
                    retimes: span.retimes,
                    content: span.content,
                })
            })
            .collect::<Result<_, PlanError>>()?,
        lookup: query.lookup,
        work: query.work,
    })
}
