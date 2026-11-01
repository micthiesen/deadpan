//! Bounded admitted construction for concrete non-Structural Signal providers.
use super::*;

use std::ops::Range;
use std::sync::Arc;

use super::super::source_voice::SourceVoiceProvider;
use crate::plan::audio_owners::admission::QueryCheckpoint;

type SpanResult<'plan, A> = QueryResult<
    GuardedBound<
        AudioSignalSpan<'plan>,
        <A as AudioSoundContextAdmission>::Issuer,
        <A as AudioSoundContextAdmission>::Lease,
    >,
    A,
>;

pub(super) fn provider_child_query<'plan, A: AudioSoundContextAdmission>(
    signal: &AudioSignal<'plan>,
    samples: Range<SignalSample>,
    limits: AudioQueryLimits,
    rule: AudioBoundaryRule,
    stops: (bool, bool),
    budget: &mut QueryBudget<'_, A>,
) -> SignalResult<'plan, A> {
    if matches!(signal.provider, SignalProvider::Structural) {
        return signal.structural_child_query(samples, limits, rule, stops, budget);
    }

    // Keep the ordinary validation order: limits, owned-input rejection,
    // transform/grid and support range all precede provider construction.
    let grid = signal.structural_grid(&samples, limits, rule)?;
    let start = budget.checkpoint();
    if limits.maximum_work != start.remaining() {
        return Err(PlanError::InvalidPlan("processing query work provenance mismatch").into());
    }
    let mut accumulator = collection::empty(budget);
    match &signal.provider {
        SignalProvider::Source { recipe, .. } => {
            let mut cursor = samples.start;
            while cursor < samples.end {
                if accumulator.spans.len() == limits.maximum_spans {
                    return Err(PlanError::AudioQueryLimit("span count").into());
                }
                budget.canonical(1)?;
                let span = source_voice_span(signal, recipe, cursor, grid, budget)?;
                let end = span.allocated_samples.end.min(samples.end);
                let span = span.map(budget, |mut span| {
                    span.samples = cursor..end;
                    span
                })?;
                cursor = end;
                accumulator = collection::push(accumulator, span, budget)?;
            }
        }
        SignalProvider::Silence => {
            let samples_allocated =
                grid.boundary(signal.support.start)?..grid.boundary(signal.support.end)?;
            let mut cursor = samples.start;
            while cursor < samples.end {
                if accumulator.spans.len() == limits.maximum_spans {
                    return Err(PlanError::AudioQueryLimit("span count").into());
                }
                budget.canonical(1)?;
                let span = silence_span(signal, grid, samples_allocated.clone(), budget)?;
                let end = span.allocated_samples.end.min(samples.end);
                let span = span.map(budget, |mut span| {
                    span.samples = cursor..end;
                    span
                })?;
                cursor = end;
                accumulator = collection::push(accumulator, span, budget)?;
            }
        }
        SignalProvider::Projected(projection) => {
            // Projected query work is one canonical unit even when empty.
            budget.canonical(1)?;
            if samples.start < samples.end {
                let span = projected_span(signal, projection, samples.start, grid, budget)?;
                let end = span.allocated_samples.end.min(samples.end);
                let span = span.map(budget, |mut span| {
                    span.samples = samples.start..end;
                    span
                })?;
                accumulator = collection::push(accumulator, span, budget)?;
            }
        }
        SignalProvider::RequiresOwnedInput => {
            unreachable!("structural_grid rejects owned Preserve input first")
        }
        SignalProvider::Structural => unreachable!("Structural returned above"),
    }

    finish_query(signal, samples, limits, accumulator, start, budget)
}

fn finish_query<'plan, A: AudioSoundContextAdmission>(
    signal: &AudioSignal<'plan>,
    samples: Range<SignalSample>,
    limits: AudioQueryLimits,
    accumulator: collection::Collected<AudioSignalSpan<'plan>, A>,
    start: QueryCheckpoint,
    budget: &mut QueryBudget<'_, A>,
) -> SignalResult<'plan, A> {
    let mut header = Owned {
        value: (None, None, None),
        memory: budget.memory(),
    };
    header.value.0 = Some(budget.project(&mut header.memory, &signal.plan.metadata.project_id)?);
    header.value.1 = Some(budget.revision(&mut header.memory, &signal.plan.metadata.revision_id)?);
    header.value.2 = budget.definition(&mut header.memory, &signal.definition)?;
    let header =
        GuardedBound::plain(header.value, budget.issuer).adopt_memory(header.memory, budget)?;
    let usage = budget.usage_since(start)?;
    let work = usage.work;
    if limits.maximum_work.checked_sub(budget.remaining) != Some(work) {
        return Err(PlanError::InvalidPlan("processing query work provenance mismatch").into());
    }
    accumulator
        .zip(header, budget)?
        .map(budget, |(accumulator, header)| SignalBuffer {
            query: AudioSignalQuery {
                project_id: header.0.expect("completed signal project header"),
                revision_id: header.1.expect("completed signal revision header"),
                definition: header.2,
                samples,
                spans: accumulator.spans,
                lookup: usage.lookup,
                work,
            },
            capacity: accumulator.capacity,
        })
}

fn source_voice_span<'plan, A: AudioSoundContextAdmission>(
    signal: &AudioSignal<'plan>,
    provider: &SourceVoiceProvider,
    sample: SignalSample,
    grid: AudioSampleGrid<SignalSample>,
    budget: &mut QueryBudget<'_, A>,
) -> SpanResult<'plan, A> {
    let audio = provider.compiled_audio();
    let transform = signal.transform()?;
    let start = transform.signal_from_local(audio.selection.start)?;
    let end = transform.signal_from_local(audio.selection.end)?;
    let mut extent = signal.support.clone();
    let mut memory = budget.memory();
    let content = if audio.selection.start == audio.selection.end {
        AudioContent::Silence {
            reason: audio.outside_reason(),
        }
    } else if sample < grid.boundary(start)? {
        extent.end = minimum(extent.end, start)?;
        AudioContent::Silence {
            reason: audio.outside_reason(),
        }
    } else if sample >= grid.boundary(end)? {
        extent.start = maximum(extent.start, end)?;
        AudioContent::Silence {
            reason: audio.outside_reason(),
        }
    } else {
        extent = intersect(extent, start..end)?;
        let support = if signal.constrain_support {
            intersect(
                signal
                    .sampling_support
                    .clone()
                    .unwrap_or_else(|| signal.support.clone()),
                start..end,
            )?
        } else {
            start..end
        };
        AudioContent::Source {
            source: budget.source(&mut memory, &audio.source)?,
            start: audio.start,
            duration: audio.duration,
            support: SourceSamplingSupport::from_local(
                &audio.source,
                audio.start,
                audio.duration,
                transform.local_at_signal_frame(support.start)?
                    ..transform.local_at_signal_frame(support.end)?,
            )?,
        }
    };

    let allocated_samples = grid.boundary(extent.start)?..grid.boundary(extent.end)?;
    if !allocated_samples.contains(&sample) {
        return Err(PlanError::InvalidPlan("source voice interval did not advance").into());
    }
    let definition = budget.definition(&mut memory, &signal.definition)?;
    let sampling = AudioSampleMap::new(
        allocated_samples.start,
        transform.local_at(allocated_samples.start)?,
        transform
            .signal_frames_per_sample
            .checked_div(transform.signal_frames_per_local_frame)?,
    )?;
    let node = budget.node(&mut memory, &signal.plan.nodes[signal.root].inspection.id)?;
    let mut repeats_capacity = budget.memory();
    let repeats = budget.repeats(&mut memory, &mut repeats_capacity, &signal.repeats)?;
    budget.merge_checked(&mut memory, &mut repeats_capacity)?;
    let gap_after = signal
        .gap
        .as_ref()
        .and_then(|gap| gap.after.as_ref())
        .map(|value| budget.iteration(&mut memory, value))
        .transpose()?;
    let span = AudioSignalSpan {
        definition,
        samples: allocated_samples.clone(),
        allocated_samples,
        signal_extent: extent,
        instance: InstancePath { node, repeats },
        gap_after,
        transform,
        grid,
        sampling,
        retimes: Vec::new(),
        content: AudioSignalContent::Leaf(content),
    };
    GuardedBound::plain(span, budget.issuer).adopt_memory(memory, budget)
}

fn silence_span<'plan, A: AudioSoundContextAdmission>(
    signal: &AudioSignal<'plan>,
    grid: AudioSampleGrid<SignalSample>,
    samples: Range<SignalSample>,
    budget: &mut QueryBudget<'_, A>,
) -> SpanResult<'plan, A> {
    let transform = signal.transform()?;
    let mut memory = budget.memory();
    let definition = budget.definition(&mut memory, &signal.definition)?;
    let node = budget.node(&mut memory, &signal.plan.nodes[signal.root].inspection.id)?;
    let mut repeats_capacity = budget.memory();
    let repeats = budget.repeats(&mut memory, &mut repeats_capacity, &signal.repeats)?;
    budget.merge_checked(&mut memory, &mut repeats_capacity)?;
    let sampling = AudioSampleMap::new(
        samples.start,
        transform.local_at(samples.start)?,
        transform
            .signal_frames_per_sample
            .checked_div(transform.signal_frames_per_local_frame)?,
    )?;
    let span = AudioSignalSpan {
        definition,
        samples: samples.clone(),
        allocated_samples: samples,
        signal_extent: signal.support.clone(),
        instance: InstancePath { node, repeats },
        gap_after: None,
        transform,
        grid,
        sampling,
        retimes: Vec::new(),
        content: AudioSignalContent::Leaf(AudioContent::Silence {
            reason: SilenceReason::NoSourceAudio,
        }),
    };
    GuardedBound::plain(span, budget.issuer).adopt_memory(memory, budget)
}

fn projected_span<'plan, A: AudioSoundContextAdmission>(
    signal: &AudioSignal<'plan>,
    projection: &Arc<crate::AudioStageProjection<'plan>>,
    sample: SignalSample,
    grid: AudioSampleGrid<SignalSample>,
    budget: &mut QueryBudget<'_, A>,
) -> SpanResult<'plan, A> {
    let transform = signal.transform()?;
    let allocated_samples =
        grid.boundary(signal.support.start)?..grid.boundary(signal.support.end)?;
    if !allocated_samples.contains(&sample) {
        return Err(PlanError::InvalidPlan("projected signal interval did not advance").into());
    }
    let sampling = AudioSampleMap::new(
        allocated_samples.start,
        transform.local_at(allocated_samples.start)?,
        transform
            .signal_frames_per_sample
            .checked_div(transform.signal_frames_per_local_frame)?,
    )?;
    let mut memory = budget.memory();
    let definition = budget.definition(&mut memory, &signal.definition)?;
    let instance = budget.instance_fixed(&mut memory, &projection.stage().descriptor().instance)?;
    budget.additional(1)?;
    let content = AudioSignalContent::ProjectedStage(Arc::clone(projection));
    let span = AudioSignalSpan {
        definition,
        samples: allocated_samples.clone(),
        allocated_samples,
        signal_extent: signal.support.clone(),
        instance,
        gap_after: None,
        transform,
        grid,
        sampling,
        retimes: Vec::new(),
        content,
    };
    GuardedBound::plain(span, budget.issuer).adopt_memory(memory, budget)
}
