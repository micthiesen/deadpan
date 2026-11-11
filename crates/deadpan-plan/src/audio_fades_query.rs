//! Actual Root and physical-Domain creative fade construction with guarded storage.
use super::*;
use crate::AudioSoundContextAdmission;
use crate::audio_allocated::admission::array;
use crate::plan::audio::Budget as LocalBudget;
use crate::plan::audio::processing_query as collection;
use crate::plan::audio_bound::query::GuardedBound;
use crate::plan::audio_owners::admission::{
    Budget as QueryBudget, Memory, Owned, Result as QueryResult, outer,
};

type Guard<T, A> = GuardedBound<
    T,
    <A as AudioSoundContextAdmission>::Issuer,
    <A as AudioSoundContextAdmission>::Lease,
>;
type Kept<T, A> =
    Owned<T, <A as AudioSoundContextAdmission>::Issuer, <A as AudioSoundContextAdmission>::Lease>;

struct FadeBuffer<I, L> {
    query: AudioFadeQuery,
    _capacity: Memory<I, L>,
}

/// Immutable actual creative fades and their independent construction leases.
pub struct AdmittedAudioFadeQuery<I, L> {
    value: GuardedBound<FadeBuffer<I, L>, I, L>,
    canonical: usize,
    additional: usize,
    forwarded: bool,
}
impl<I: Copy + Eq, L> AdmittedAudioFadeQuery<I, L> {
    pub fn query(&self) -> &AudioFadeQuery {
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

pub(in crate::plan) fn ordinary<A: AudioSoundContextAdmission>(
    query: AudioFadeQuery,
    admission: &A,
) -> QueryResult<AdmittedAudioFadeQuery<A::Issuer, A::Lease>, A> {
    if A::METERED {
        return Err(PlanError::InvalidPlan("fade accounting mode mismatch").into());
    }
    let canonical = query.work;
    let issuer = admission.issuer();
    Ok(AdmittedAudioFadeQuery {
        value: GuardedBound::plain(
            FadeBuffer {
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

impl RenderPlan {
    pub fn audio_fades_admitted<A: AudioSoundContextAdmission>(
        &self,
        samples: Range<AudioSample>,
        limits: AudioQueryLimits,
        admission: &mut A,
    ) -> Result<AdmittedAudioFadeQuery<A::Issuer, A::Lease>, A::Error> {
        if !A::METERED {
            let query = self.audio_fades(samples, limits).map_err(A::Error::from)?;
            return outer::<_, A>(ordinary(query, admission));
        }
        limits.validate()?;
        if samples.start.0 < 0
            || samples.end < samples.start
            || samples.end > self.audio_duration()?
        {
            return Err(PlanError::AudioRangeOutOfRange.into());
        }
        construct(self, None, samples, limits, admission)
    }
}
impl AudioDomain<'_> {
    pub fn fades_admitted<A: AudioSoundContextAdmission>(
        &self,
        samples: Range<AudioSample>,
        limits: AudioQueryLimits,
        admission: &mut A,
    ) -> Result<AdmittedAudioFadeQuery<A::Issuer, A::Lease>, A::Error> {
        if !A::METERED {
            let query = self.fades(samples, limits).map_err(A::Error::from)?;
            return outer::<_, A>(ordinary(query, admission));
        }
        limits.validate()?;
        if samples.start < self.samples.start
            || samples.end < samples.start
            || samples.end > self.samples.end
        {
            return Err(PlanError::AudioRangeOutOfRange.into());
        }
        construct(self.plan, Some(&self.seed), samples, limits, admission)
    }
}

fn construct<A: AudioSoundContextAdmission>(
    plan: &RenderPlan,
    seed: Option<&AudioWalkSeed>,
    samples: Range<AudioSample>,
    limits: AudioQueryLimits,
    admission: &mut A,
) -> Result<AdmittedAudioFadeQuery<A::Issuer, A::Lease>, A::Error> {
    let mut local = LocalBudget {
        remaining: limits.maximum_work,
        lookup: LookupStats::default(),
    };
    let mut budget = QueryBudget::new(&mut local, admission);
    outer::<_, A>((|| {
        let mut spans = collection::empty(&budget);
        let mut cursor = samples.start;
        while cursor < samples.end {
            if spans.spans.len() == limits.maximum_spans {
                return Err(PlanError::AudioQueryLimit("span count").into());
            }
            budget.canonical(1)?;
            let mut captured = Owned {
                value: EditorialCapture::default(),
                memory: budget.memory(),
            };
            let walked = plan.audio_walk_inner_admitted(
                cursor,
                &mut budget,
                seed,
                true,
                true,
                (
                    None,
                    plan.has_audio_editorial_edges.then_some(&mut captured),
                    None,
                ),
            )?;
            let value = walked.value.adopt_memory(walked.memory, &mut budget)?;
            let span = match &*value {
                AudioWalkSpan::Leaf(leaf) => {
                    let grid = leaf.grid;
                    let voice = ExactRatio::integer(leaf.envelope_samples.start.0)
                        ..ExactRatio::integer(leaf.envelope_samples.end.0);
                    let span = ordinary_leaf(value, cursor, samples.end, &mut budget)?;
                    add_editorial(plan, span, grid, voice, &captured, &mut budget)?
                }
                AudioWalkSpan::Stage(processing) => match &processing.content {
                    AudioSignalContent::Stage(_) | AudioSignalContent::ProjectedStage(_) => {
                        let allocated_end = processing.allocated_samples.end;
                        let flat = plan.audio_walk_inner_admitted(
                            cursor,
                            &mut budget,
                            seed,
                            false,
                            false,
                            (
                                None,
                                plan.has_audio_editorial_edges.then_some(&mut captured),
                                None,
                            ),
                        )?;
                        if !matches!(&*flat.value, AudioWalkSpan::Leaf(_)) {
                            return Err(
                                PlanError::InvalidPlan("flattened fade retained a stage").into()
                            );
                        }
                        let flat = flat.flattened_leaf(&mut budget)?;
                        let grid = flat.grid;
                        let voice = ExactRatio::integer(flat.envelope_samples.start.0)
                            ..ExactRatio::integer(flat.envelope_samples.end.0);
                        let flat = GuardedBound::plain(flat.value, budget.issuer)
                            .adopt_memory(flat.memory, &mut budget)?;
                        let span = ordinary_span(
                            flat,
                            cursor,
                            samples.end.min(allocated_end),
                            &mut budget,
                        )?;
                        let span = add_editorial(plan, span, grid, voice, &captured, &mut budget)?;
                        span.keep(value, &mut budget)?
                    }
                    AudioSignalContent::Leaf(_) => {
                        return Err(
                            PlanError::InvalidPlan("fade processing retained a leaf").into()
                        );
                    }
                },
            };
            if span.value.samples.start != cursor || span.value.samples.end <= cursor {
                return Err(PlanError::InvalidPlan("fade interval did not advance").into());
            }
            cursor = span.value.samples.end;
            let finished = span.finish(&mut budget)?;
            spans = collection::push(spans, finished, &mut budget)?;
        }
        finish_query(spans, limits.maximum_work, &mut budget)
    })())
}

pub(in crate::plan) fn finish_query<A: AudioSoundContextAdmission>(
    spans: collection::Collected<AudioFadeSpan, A>,
    maximum_work: usize,
    budget: &mut QueryBudget<'_, A>,
) -> QueryResult<AdmittedAudioFadeQuery<A::Issuer, A::Lease>, A> {
    budget.check()?;
    let canonical = budget.canonical_used;
    let additional = budget.additional_used;
    let work = canonical
        .checked_add(additional)
        .ok_or(TimeError::Overflow)?;
    if maximum_work.checked_sub(budget.remaining) != Some(work) {
        return Err(PlanError::InvalidPlan("fade query work provenance mismatch").into());
    }
    let lookup = budget.lookup;
    let value = spans.map(budget, |spans| FadeBuffer {
        query: AudioFadeQuery {
            spans: spans.spans,
            lookup,
            work,
        },
        _capacity: spans.capacity,
    })?;
    Ok(AdmittedAudioFadeQuery {
        value,
        canonical,
        additional,
        forwarded: true,
    })
}

struct Retained {
    samples: Range<AudioSample>,
    length: u64,
    progress: ExactRatio,
    extent: Range<ExactRatio>,
    boundaries: AudioBoundaries,
}
struct SpanWork<I, L> {
    value: GuardedBound<AudioFadeSpan, I, L>,
    start_capacity: Memory<I, L>,
    end_capacity: Memory<I, L>,
}
impl<I: Copy + Eq, L> SpanWork<I, L> {
    fn finish<A: AudioSoundContextAdmission<Issuer = I, Lease = L>>(
        self,
        budget: &mut QueryBudget<'_, A>,
    ) -> QueryResult<Guard<AudioFadeSpan, A>, A> {
        let value = self.value.adopt_memory(self.start_capacity, budget)?;
        value.adopt_memory(self.end_capacity, budget)
    }
    fn keep<T, A: AudioSoundContextAdmission<Issuer = I, Lease = L>>(
        self,
        other: Guard<T, A>,
        budget: &mut QueryBudget<'_, A>,
    ) -> QueryResult<Self, A> {
        let value = self
            .value
            .zip(other, budget)?
            .map(budget, |(span, _)| span)?;
        Ok(Self {
            value,
            start_capacity: self.start_capacity,
            end_capacity: self.end_capacity,
        })
    }
}
fn ordinary_leaf<A: AudioSoundContextAdmission>(
    leaf: Guard<AudioWalkSpan<'_>, A>,
    start: AudioSample,
    end: AudioSample,
    budget: &mut QueryBudget<'_, A>,
) -> QueryResult<SpanWork<A::Issuer, A::Lease>, A> {
    let AudioWalkSpan::Leaf(value) = &*leaf else {
        return Err(PlanError::InvalidPlan("flattened fade retained a stage").into());
    };
    let samples = start..end.min(value.allocated_samples.end);
    let length = value.envelope.length();
    let progress = ExactRatio::new(value.envelope.progress_at(start)?, 1)?;
    let retained = leaf.map(budget, move |leaf| {
        let AudioWalkSpan::Leaf(leaf) = leaf else {
            unreachable!("observed fade leaf")
        };
        Retained {
            samples,
            length,
            progress,
            extent: leaf.envelope_extent,
            boundaries: leaf.boundaries,
        }
    })?;
    retained_span(retained, budget)
}
fn ordinary_span<A: AudioSoundContextAdmission>(
    leaf: Guard<AudioSpan, A>,
    start: AudioSample,
    end: AudioSample,
    budget: &mut QueryBudget<'_, A>,
) -> QueryResult<SpanWork<A::Issuer, A::Lease>, A> {
    let samples = start..end.min(leaf.allocated_samples.end);
    let length = leaf.envelope.length();
    let progress = ExactRatio::new(leaf.envelope.progress_at(start)?, 1)?;
    let retained = leaf.map(budget, move |leaf| Retained {
        samples,
        length,
        progress,
        extent: leaf.envelope_extent,
        boundaries: leaf.boundaries,
    })?;
    retained_span(retained, budget)
}

struct StartSide {
    samples: Range<AudioSample>,
    length: u64,
    progress: ExactRatio,
    end_at: ExactRatio,
    start: Vec<AudioFadeEdge>,
    end_origins: Vec<AudioBoundaryOrigin>,
}
fn retained_span<A: AudioSoundContextAdmission>(
    value: Guard<Retained, A>,
    budget: &mut QueryBudget<'_, A>,
) -> QueryResult<SpanWork<A::Issuer, A::Lease>, A> {
    let mut start = Owned {
        value: Vec::new(),
        memory: budget.memory(),
    };
    budget.reserve(&mut start.memory, array::<AudioFadeEdge>(1)?)?;
    start.value = Vec::with_capacity(1);
    assert_eq!(start.value.capacity(), 1);
    let partial = value.map(budget, move |value| {
        let mut edges = start.value;
        edges.push(AudioFadeEdge {
            at: value.extent.start,
            length: value.length,
            distance_at_start: value.progress,
            origins: value.boundaries.start,
            editorial: false,
        });
        StartSide {
            samples: value.samples,
            length: value.length,
            progress: value.progress,
            end_at: value.extent.end,
            start: edges,
            end_origins: value.boundaries.end,
        }
    })?;
    // Keep the first side allocated before the original opposite-side arithmetic.
    let distance =
        ExactRatio::new(i128::from(partial.length) - 1, 1)?.checked_sub(partial.progress)?;
    let mut end = Owned {
        value: Vec::new(),
        memory: budget.memory(),
    };
    budget.reserve(&mut end.memory, array::<AudioFadeEdge>(1)?)?;
    end.value = Vec::with_capacity(1);
    assert_eq!(end.value.capacity(), 1);
    let value = partial.map(budget, move |partial| {
        let mut edges = end.value;
        edges.push(AudioFadeEdge {
            at: partial.end_at,
            length: partial.length,
            distance_at_start: distance,
            origins: partial.end_origins,
            editorial: false,
        });
        AudioFadeSpan {
            samples: partial.samples,
            start: partial.start,
            end: edges,
        }
    })?;
    Ok(SpanWork {
        value,
        start_capacity: start.memory,
        end_capacity: end.memory,
    })
}

struct OriginList<I, L> {
    value: Vec<AudioBoundaryOrigin>,
    memory: Memory<I, L>,
    capacity: Memory<I, L>,
}
struct EdgeWork<I, L> {
    value: AudioFadeEdge,
    memory: Memory<I, L>,
    capacity: Memory<I, L>,
}
fn push_origin<A: AudioSoundContextAdmission>(
    output: &mut Vec<AudioBoundaryOrigin>,
    memory: &mut Memory<A::Issuer, A::Lease>,
    capacity: &mut Memory<A::Issuer, A::Lease>,
    mut origin: Kept<AudioBoundaryOrigin, A>,
    budget: &mut QueryBudget<'_, A>,
) -> QueryResult<(), A> {
    budget.check_memory(memory)?;
    budget.check_memory(capacity)?;
    budget.check_memory(&origin.memory)?;
    let needed = output.len().checked_add(1).ok_or(TimeError::Overflow)?;
    budget.grow(output, capacity, needed)?;
    // Transfer the owning edge's complete payload lane before the infallible move.
    budget.check_memory(&origin.memory)?;
    budget.check_memory(memory)?;
    budget.merge_checked(memory, &mut origin.memory)?;
    budget.check_memory(memory)?;
    output.push(origin.value);
    Ok(())
}

fn append_origins<A: AudioSoundContextAdmission>(
    plan: &RenderPlan,
    captured: &EditorialCapture,
    constraint: &EnvelopeConstraint,
    at: ExactRatio,
    output: &mut OriginList<A::Issuer, A::Lease>,
    budget: &mut QueryBudget<'_, A>,
) -> QueryResult<(), A> {
    budget.canonical(1)?;
    let node = &plan.nodes[constraint.node];
    let owner = BoundaryOwner {
        node: &node.inspection.id,
        repeats: captured
            .repeats
            .get(..constraint.repeat_count)
            .ok_or(PlanError::InvalidPlan("editorial boundary occurrence"))?,
        gap_after: constraint.gap_after.as_ref(),
        policies: if constraint.placement_support {
            Default::default()
        } else {
            node.audio_edges
        },
        placement_support: constraint.placement_support,
    };
    for (coordinate, kind) in [
        (constraint.range.start, constraint.kinds.0),
        (constraint.range.end, constraint.kinds.1),
    ] {
        if coordinate == at {
            let origin = owner.capture_admitted(kind, budget)?;
            budget.canonical(output.value.len())?;
            if !output.value.contains(&origin.value) {
                push_origin(
                    &mut output.value,
                    &mut output.memory,
                    &mut output.capacity,
                    origin,
                    budget,
                )?;
            }
        }
    }
    Ok(())
}

struct OriginCopy<I, L> {
    instance: Option<deadpan_core::InstancePath>,
    gap_after: Option<deadpan_core::IterationId>,
    memory: Memory<I, L>,
    capacity: Memory<I, L>,
}
fn copy_origin<A: AudioSoundContextAdmission>(
    origin: &AudioBoundaryOrigin,
    budget: &mut QueryBudget<'_, A>,
) -> QueryResult<Kept<AudioBoundaryOrigin, A>, A> {
    budget.additional(1)?;
    let mut copy = OriginCopy {
        instance: None,
        gap_after: None,
        memory: budget.memory(),
        capacity: budget.memory(),
    };
    copy.instance =
        Some(budget.instance(&mut copy.memory, &mut copy.capacity, &origin.instance)?);
    copy.gap_after = origin
        .gap_after
        .as_ref()
        .map(|value| budget.iteration(&mut copy.memory, value))
        .transpose()?;
    let mut result = Owned {
        value: AudioBoundaryOrigin {
            instance: copy.instance.take().expect("copied boundary instance"),
            gap_after: copy.gap_after.take(),
            kind: origin.kind,
            placement_support: origin.placement_support,
            policy: origin.policy,
        },
        memory: copy.memory,
    };
    budget.check_memory(&result.memory)?;
    budget.check_memory(&copy.capacity)?;
    budget.merge_checked(&mut result.memory, &mut copy.capacity)?;
    budget.check_memory(&result.memory)?;
    Ok(result)
}

struct SideGrowth {
    span: AudioFadeSpan,
    old: std::vec::IntoIter<AudioFadeEdge>,
    replacement: Vec<AudioFadeEdge>,
}
struct GrowingSide<I, L> {
    value: GuardedBound<SideGrowth, I, L>,
    start_capacity: Memory<I, L>,
    end_capacity: Memory<I, L>,
    next_capacity: Memory<I, L>,
}
fn side(span: &AudioFadeSpan, start: bool) -> &Vec<AudioFadeEdge> {
    if start { &span.start } else { &span.end }
}
fn side_mut(span: &mut AudioFadeSpan, start: bool) -> &mut Vec<AudioFadeEdge> {
    if start {
        &mut span.start
    } else {
        &mut span.end
    }
}
fn grow_side<A: AudioSoundContextAdmission>(
    span: SpanWork<A::Issuer, A::Lease>,
    start: bool,
    budget: &mut QueryBudget<'_, A>,
) -> QueryResult<SpanWork<A::Issuer, A::Lease>, A> {
    let edges = side(&span.value, start);
    let needed = edges.len().checked_add(1).ok_or(TimeError::Overflow)?;
    budget.check_memory(&span.start_capacity)?;
    budget.check_memory(&span.end_capacity)?;
    if needed <= edges.capacity() {
        return Ok(span);
    }
    let count = needed
        .max(edges.capacity().checked_mul(2).ok_or(TimeError::Overflow)?)
        .max(1);
    let mut replacement = Owned {
        value: Vec::new(),
        memory: budget.memory(),
    };
    budget.reserve(&mut replacement.memory, array::<AudioFadeEdge>(count)?)?;
    replacement.value = Vec::with_capacity(count);
    assert_eq!(replacement.value.capacity(), count);
    let value = span.value.map(budget, move |mut span| {
        let old = std::mem::take(side_mut(&mut span, start)).into_iter();
        SideGrowth {
            span,
            old,
            replacement: replacement.value,
        }
    })?;
    let mut growth = GrowingSide {
        value,
        start_capacity: span.start_capacity,
        end_capacity: span.end_capacity,
        next_capacity: replacement.memory,
    };
    while !growth.value.old.as_slice().is_empty() {
        budget.additional(1)?;
        growth.value = growth.value.map(budget, |mut value| {
            value
                .replacement
                .push(value.old.next().expect("observed fade edge"));
            value
        })?;
    }
    let value = growth.value.map(budget, |growth| {
        let SideGrowth {
            mut span,
            old,
            replacement,
        } = growth;
        drop(old);
        *side_mut(&mut span, start) = replacement;
        span
    })?;
    if start {
        growth.start_capacity = growth.next_capacity;
    } else {
        growth.end_capacity = growth.next_capacity;
    }
    Ok(SpanWork {
        value,
        start_capacity: growth.start_capacity,
        end_capacity: growth.end_capacity,
    })
}

fn merge_edge<A: AudioSoundContextAdmission>(
    mut span: SpanWork<A::Issuer, A::Lease>,
    start: bool,
    mut new: EdgeWork<A::Issuer, A::Lease>,
    budget: &mut QueryBudget<'_, A>,
) -> QueryResult<SpanWork<A::Issuer, A::Lease>, A> {
    budget.canonical(side(&span.value, start).len() + 1)?;
    let index = side(&span.value, start)
        .iter()
        .position(|old| old.at == new.value.at);
    if let Some(index) = index {
        let old = &side(&span.value, start)[index];
        if old.editorial {
            new.value.length = new.value.length.min(old.length);
        }
        for origin in &old.origins {
            budget.canonical(new.value.origins.len() + origin.instance.repeats.len() + 1)?;
            if !new.value.origins.contains(origin) {
                let copy = copy_origin(origin, budget)?;
                push_origin(
                    &mut new.value.origins,
                    &mut new.memory,
                    &mut new.capacity,
                    copy,
                    budget,
                )?;
            }
        }
    } else {
        span = grow_side(span, start, budget)?;
    }
    let edge = GuardedBound::plain(new.value, budget.issuer)
        .adopt_memory(new.memory, budget)?
        .adopt_memory(new.capacity, budget)?;
    let value = span
        .value
        .zip(edge, budget)?
        .map(budget, |(mut span, edge)| {
            let edges = side_mut(&mut span, start);
            if let Some(index) = index {
                edges[index] = edge;
            } else {
                edges.push(edge);
            }
            span
        })?;
    Ok(SpanWork {
        value,
        start_capacity: span.start_capacity,
        end_capacity: span.end_capacity,
    })
}

fn add_editorial<A: AudioSoundContextAdmission>(
    plan: &RenderPlan,
    mut span: SpanWork<A::Issuer, A::Lease>,
    grid: crate::AudioSampleGrid<AudioSample>,
    voice: Range<ExactRatio>,
    captured: &Kept<EditorialCapture, A>,
    budget: &mut QueryBudget<'_, A>,
) -> QueryResult<SpanWork<A::Issuer, A::Lease>, A> {
    let Some(retained) = span.value.start.first() else {
        return Ok(span);
    };
    let voice_extent = retained.at
        ..span
            .value
            .end
            .first()
            .ok_or(PlanError::InvalidPlan(
                "retained fade is missing its opposite edge",
            ))?
            .at;
    for marker in &captured.value.editorial {
        budget.canonical(1)?;
        let range = &marker.boundary.range;
        let first = grid.boundary(range.start)?;
        let last = grid.boundary(range.end)?;
        let intersection_start =
            crate::plan::audio::maximum(voice.start, ExactRatio::integer(first.0))?;
        let intersection_end = crate::plan::audio::minimum(voice.end, ExactRatio::integer(last.0))?;
        if intersection_end.compare(intersection_start).is_le() {
            continue;
        }
        let length = u64::try_from(intersection_end.ceil()? - intersection_start.ceil()?)
            .map_err(|_| TimeError::Overflow)?;
        for (active, start_side, at, boundary) in [
            (marker.edges.start, true, range.start, first),
            (marker.edges.end, false, range.end, last),
        ] {
            let incident = if let Some((first, last)) = marker.incident_samples {
                let sample = if start_side { first } else { last };
                sample.compare(voice.start).is_ge() && sample.compare(voice.end).is_lt()
            } else if start_side {
                at.compare(voice_extent.start).is_ge() && at.compare(voice_extent.end).is_lt()
            } else {
                at.compare(voice_extent.start).is_gt() && at.compare(voice_extent.end).is_le()
            };
            if !active || !incident {
                continue;
            }
            let mut origins = OriginList {
                value: Vec::new(),
                memory: budget.memory(),
                capacity: budget.memory(),
            };
            for constraint in captured
                .value
                .constraints
                .iter()
                .chain(captured.value.editorial.iter().map(|value| &value.boundary))
            {
                append_origins(plan, &captured.value, constraint, at, &mut origins, budget)?;
            }
            let distance = if start_side {
                i128::from(span.value.samples.start.0) - i128::from(boundary.0)
            } else {
                i128::from(boundary.0) - 1 - i128::from(span.value.samples.start.0)
            };
            let new = EdgeWork {
                value: AudioFadeEdge {
                    at,
                    length,
                    distance_at_start: ExactRatio::new(distance, 1)?,
                    origins: origins.value,
                    editorial: true,
                },
                memory: origins.memory,
                capacity: origins.capacity,
            };
            span = merge_edge(span, start_side, new, budget)?;
        }
    }
    Ok(span)
}
