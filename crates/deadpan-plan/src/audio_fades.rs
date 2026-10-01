//! Creative edge envelopes on the consuming output clock. Raw endpoint and
//! explicit-silence policies remain the responsibility of the audio readers.

use std::ops::Range;

use deadpan_core::{AudioSample, ExactRatio, TimeError};
use serde::Serialize;

use super::audio::{AudioWalkSpan, Budget, EditorialCapture, EnvelopeConstraint};
use super::audio_boundary::BoundaryOwner;
use super::audio_domain::AudioWalkSeed;
use super::{
    AudioBoundaries, AudioBoundaryOrigin, AudioDomain, AudioProcessingSpan, AudioQueryLimits,
    AudioSignalContent, AudioSpan, LookupStats, PlanError, RenderPlan,
};

/// One side of a creative envelope. Start distance increases by one per output
/// sample; end distance decreases. Retained opposite sides keep their own width
/// and exact phase when a new editorial boundary is authored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AudioFadeEdge {
    pub at: ExactRatio,
    pub length: u64,
    pub distance_at_start: ExactRatio,
    pub origins: Vec<AudioBoundaryOrigin>,
    pub editorial: bool,
}

/// Distinct active edges combine by minimum gain, never multiplication.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AudioFadeSpan {
    pub samples: Range<AudioSample>,
    pub start: Vec<AudioFadeEdge>,
    pub end: Vec<AudioFadeEdge>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AudioFadeQuery {
    pub spans: Vec<AudioFadeSpan>,
    pub lookup: LookupStats,
    #[serde(skip)]
    pub work: usize,
}

impl RenderPlan {
    /// Derive creative edge envelopes without preparing or fading PCM. A bound
    /// physical output keeps its reference phase; Preserve input bindings never
    /// turn an input-clock envelope into a post-stretch output fade.
    pub fn audio_fades(
        &self,
        samples: Range<AudioSample>,
        limits: AudioQueryLimits,
    ) -> Result<AudioFadeQuery, PlanError> {
        limits.validate()?;
        if samples.start.0 < 0
            || samples.end < samples.start
            || samples.end > self.audio_duration()?
        {
            return Err(PlanError::AudioRangeOutOfRange);
        }
        fade_query(self, samples, limits, None)
    }
}

impl AudioDomain<'_> {
    /// Query the same output-envelope semantics in a signed physical domain.
    pub fn fades(
        &self,
        samples: Range<AudioSample>,
        limits: AudioQueryLimits,
    ) -> Result<AudioFadeQuery, PlanError> {
        limits.validate()?;
        if samples.start < self.samples.start
            || samples.end < samples.start
            || samples.end > self.samples.end
        {
            return Err(PlanError::AudioRangeOutOfRange);
        }
        fade_query(self.plan, samples, limits, Some(&self.seed))
    }
}

fn fade_query(
    plan: &RenderPlan,
    samples: Range<AudioSample>,
    limits: AudioQueryLimits,
    seed: Option<&AudioWalkSeed>,
) -> Result<AudioFadeQuery, PlanError> {
    let mut budget = Budget {
        remaining: limits.maximum_work,
        lookup: LookupStats::default(),
    };
    let mut spans = Vec::new();
    let mut cursor = samples.start;
    while cursor < samples.end {
        if spans.len() == limits.maximum_spans {
            return Err(PlanError::AudioQueryLimit("span count"));
        }
        budget.spend(1)?;
        let mut captured = EditorialCapture::default();
        let walked = if plan.has_audio_editorial_edges {
            plan.audio_walk_editorial(cursor, &mut budget, seed, (true, true), &mut captured)?
        } else {
            plan.audio_walk(cursor, &mut budget, seed, true, true, None)?
        };
        let span = match walked {
            AudioWalkSpan::Leaf(leaf) => {
                let grid = leaf.grid;
                let voice_samples = ExactRatio::integer(leaf.envelope_samples.start.0)
                    ..ExactRatio::integer(leaf.envelope_samples.end.0);
                let mut span = ordinary(leaf, cursor, samples.end)?;
                add_editorial(plan, &mut span, grid, voice_samples, captured, &mut budget)?;
                span
            }
            AudioWalkSpan::Stage(processing) => match &processing.content {
                AudioSignalContent::Bound(_) => {
                    bound_fade(processing, cursor, samples.end, captured, &mut budget)?
                }
                AudioSignalContent::Stage(_) | AudioSignalContent::ProjectedStage(_) => {
                    // The first opaque output owns creative fades. Flatten its
                    // current geometry in this output grid, retaining ancestor
                    // constraints and deliberately ignoring input bindings.
                    let flat = if plan.has_audio_editorial_edges {
                        plan.audio_walk_editorial(
                            cursor,
                            &mut budget,
                            seed,
                            (false, false),
                            &mut captured,
                        )?
                    } else {
                        plan.audio_walk(cursor, &mut budget, seed, false, false, None)?
                    };
                    let AudioWalkSpan::Leaf(leaf) = flat else {
                        return Err(PlanError::InvalidPlan("flattened fade retained a stage"));
                    };
                    let grid = leaf.grid;
                    let voice_samples = ExactRatio::integer(leaf.envelope_samples.start.0)
                        ..ExactRatio::integer(leaf.envelope_samples.end.0);
                    let mut span = ordinary(
                        leaf,
                        cursor,
                        samples.end.min(processing.allocated_samples.end),
                    )?;
                    add_editorial(plan, &mut span, grid, voice_samples, captured, &mut budget)?;
                    span
                }
                AudioSignalContent::Leaf(_) => {
                    return Err(PlanError::InvalidPlan("fade processing retained a leaf"));
                }
            },
        };
        if span.samples.start != cursor || span.samples.end <= cursor {
            return Err(PlanError::InvalidPlan("fade interval did not advance"));
        }
        cursor = span.samples.end;
        spans.push(span);
    }
    Ok(AudioFadeQuery {
        spans,
        lookup: budget.lookup,
        work: limits.maximum_work - budget.remaining,
    })
}

fn ordinary(
    leaf: AudioSpan,
    start: AudioSample,
    end: AudioSample,
) -> Result<AudioFadeSpan, PlanError> {
    retained(
        start..end.min(leaf.allocated_samples.end),
        leaf.envelope.length(),
        ExactRatio::new(leaf.envelope.progress_at(start)?, 1)?,
        leaf.envelope_extent,
        leaf.boundaries,
    )
}

fn bound_fade(
    processing: AudioProcessingSpan<'_>,
    start: AudioSample,
    end: AudioSample,
    outer: EditorialCapture,
    budget: &mut Budget,
) -> Result<AudioFadeSpan, PlanError> {
    let AudioSignalContent::Bound(bound) = processing.content else {
        return Err(PlanError::InvalidPlan("fade has no bound output"));
    };
    let end = end.min(processing.allocated_samples.end);
    let q = bound.reference_at_wide_offset(
        i128::from(start.0) - i128::from(processing.allocated_samples.start.0),
    )?;
    let step = bound.reference_samples_per_output_sample();
    if step.compare_integer(0).is_le() {
        return Err(PlanError::InvalidPlan(
            "fade reference rate must be positive",
        ));
    }
    let Some(mut domain) = bound.envelope_domain(budget)? else {
        return Ok(empty(start..end));
    };
    let origin = bound.lattice_origin();
    let scale = bound.envelope_scale();
    if scale.compare_integer(0).is_le() {
        return Err(PlanError::InvalidPlan(
            "fade virtual scale must be positive",
        ));
    }
    // Follow the actual retained voice, including resumed phases which straddle
    // a current structural seam. Carry only outer creative candidates into this
    // inspection domain; raw binding support and evaluation remain untouched.
    budget.spend(outer.editorial.len())?;
    let to_retained = |at: ExactRatio| {
        origin.checked_add(
            at.checked_sub(processing.transform.project_origin)?
                .checked_div(scale)?,
        )
    };
    for mut marker in outer.editorial {
        if marker.boundary.node == domain.seed.node
            && marker.boundary.repeat_count == domain.seed.repeats.len()
            && marker.boundary.gap_after.is_none()
        {
            budget.spend(marker.boundary.repeat_count)?;
            if outer.repeats.get(..marker.boundary.repeat_count)
                == Some(domain.seed.repeats.as_slice())
            {
                domain.seed.suppress_entry_editorial.start |= marker.edges.start;
                domain.seed.suppress_entry_editorial.end |= marker.edges.end;
            }
        }
        let first = processing.grid.boundary(marker.boundary.range.start)?;
        let last = processing.grid.boundary(marker.boundary.range.end)?;
        marker.incident_samples = Some((
            bound.reference_at_wide_offset(
                i128::from(first.0) - i128::from(processing.allocated_samples.start.0),
            )?,
            bound.reference_at_wide_offset(
                i128::from(last.0) - 1 - i128::from(processing.allocated_samples.start.0),
            )?,
        ));
        marker.boundary.range =
            to_retained(marker.boundary.range.start)?..to_retained(marker.boundary.range.end)?;
        domain.seed.editorial.push(marker);
    }
    let support = domain.root_samples();
    if q.compare_integer(support.start.0).is_lt() {
        return Ok(empty(
            start..inverse_end(start, end, q, step, support.start)?,
        ));
    }
    if q.compare_integer(support.end.0).is_ge() {
        return Ok(empty(start..end));
    }
    // The floor chooses a discrete reference interval only. Its fractional
    // phase remains exact below; no sample-by-sample traversal is needed.
    let probe = AudioSample(i64::try_from(q.floor()).map_err(|_| TimeError::Overflow)?);
    let mut captured = EditorialCapture::default();
    let walked = if domain.plan.has_audio_editorial_edges {
        domain.plan.audio_walk_editorial(
            probe,
            budget,
            Some(&domain.seed),
            (false, false),
            &mut captured,
        )?
    } else {
        domain
            .plan
            .audio_walk(probe, budget, Some(&domain.seed), false, false, None)?
    };
    let AudioWalkSpan::Leaf(leaf) = walked else {
        return Err(PlanError::InvalidPlan("bound fade retained a stage"));
    };
    let end = inverse_end(start, end, q, step, leaf.allocated_samples.end)?;
    let virtual_start = virtual_position(leaf.envelope_extent.start, origin, scale)?;
    let virtual_end = virtual_position(leaf.envelope_extent.end, origin, scale)?;
    let first = leaf.grid.boundary(virtual_start)?;
    let last = leaf.grid.boundary(virtual_end)?;
    let length =
        u64::try_from(i128::from(last.0) - i128::from(first.0)).map_err(|_| TimeError::Overflow)?;
    let project = |at: ExactRatio| {
        processing
            .transform
            .project_origin
            .checked_add(at.checked_sub(origin)?.checked_mul(scale)?)
    };
    let output = |sample: ExactRatio| {
        ExactRatio::integer(start.0).checked_add(sample.checked_sub(q)?.checked_div(step)?)
    };
    // New edge width follows the delivered voice interval, independently of
    // the retained envelope's historical, separately rounded virtual width.
    let voice_samples = output(ExactRatio::integer(leaf.envelope_samples.start.0))?
        ..output(ExactRatio::integer(leaf.envelope_samples.end.0))?;
    let mut span = retained(
        start..end,
        length,
        q.checked_sub(ExactRatio::integer(leaf.envelope_samples.start.0))?
            .checked_div(step)?,
        project(leaf.envelope_extent.start)?..project(leaf.envelope_extent.end)?,
        leaf.boundaries,
    )?;
    budget.spend(captured.editorial.len() + captured.constraints.len())?;
    for constraint in captured.constraints.iter_mut().chain(
        captured
            .editorial
            .iter_mut()
            .map(|marker| &mut marker.boundary),
    ) {
        constraint.range = project(constraint.range.start)?..project(constraint.range.end)?;
    }
    for marker in &mut captured.editorial {
        if let Some((first, last)) = marker.incident_samples {
            marker.incident_samples = Some((output(first)?, output(last)?));
        }
    }
    add_editorial(
        domain.plan,
        &mut span,
        processing.grid,
        voice_samples,
        captured,
        budget,
    )?;
    Ok(span)
}

fn empty(samples: Range<AudioSample>) -> AudioFadeSpan {
    AudioFadeSpan {
        samples,
        start: Vec::new(),
        end: Vec::new(),
    }
}

fn retained(
    samples: Range<AudioSample>,
    length: u64,
    progress: ExactRatio,
    extent: Range<ExactRatio>,
    boundaries: AudioBoundaries,
) -> Result<AudioFadeSpan, PlanError> {
    Ok(AudioFadeSpan {
        samples,
        start: vec![AudioFadeEdge {
            at: extent.start,
            length,
            distance_at_start: progress,
            origins: boundaries.start,
            editorial: false,
        }],
        end: vec![AudioFadeEdge {
            at: extent.end,
            length,
            distance_at_start: ExactRatio::new(i128::from(length) - 1, 1)?.checked_sub(progress)?,
            origins: boundaries.end,
            editorial: false,
        }],
    })
}

fn add_editorial(
    plan: &RenderPlan,
    span: &mut AudioFadeSpan,
    grid: crate::AudioSampleGrid<AudioSample>,
    voice_samples: Range<ExactRatio>,
    captured: EditorialCapture,
    budget: &mut Budget,
) -> Result<(), PlanError> {
    let Some(retained) = span.start.first() else {
        // Empty retained support stays empty, regardless of authored markers.
        return Ok(());
    };
    let voice_start = voice_samples.start;
    let voice_end = voice_samples.end;
    let voice_extent = retained.at
        ..span
            .end
            .first()
            .ok_or(PlanError::InvalidPlan(
                "retained fade is missing its opposite edge",
            ))?
            .at;
    for marker in &captured.editorial {
        budget.spend(1)?;
        let range = &marker.boundary.range;
        let first = grid.boundary(range.start)?;
        let last = grid.boundary(range.end)?;
        // A long marked group can expose a one-sample incident voice. Respect
        // that voice's retained audible interval without clipping raw support.
        let intersection_start = super::audio::maximum(voice_start, ExactRatio::integer(first.0))?;
        let intersection_end = super::audio::minimum(voice_end, ExactRatio::integer(last.0))?;
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
                sample.compare(voice_start).is_ge() && sample.compare(voice_end).is_lt()
            } else if start_side {
                at.compare(voice_extent.start).is_ge() && at.compare(voice_extent.end).is_lt()
            } else {
                at.compare(voice_extent.start).is_gt() && at.compare(voice_extent.end).is_le()
            };
            if !active || !incident {
                continue;
            }
            let mut origins = Vec::new();
            for constraint in captured
                .constraints
                .iter()
                .chain(captured.editorial.iter().map(|value| &value.boundary))
            {
                append_origins(plan, &captured, constraint, at, &mut origins, budget)?;
            }
            let distance = if start_side {
                i128::from(span.samples.start.0) - i128::from(boundary.0)
            } else {
                i128::from(boundary.0) - 1 - i128::from(span.samples.start.0)
            };
            merge_edge(
                if start_side {
                    &mut span.start
                } else {
                    &mut span.end
                },
                AudioFadeEdge {
                    at,
                    length,
                    distance_at_start: ExactRatio::new(distance, 1)?,
                    origins,
                    editorial: true,
                },
                budget,
            )?;
        }
    }
    Ok(())
}

fn append_origins(
    plan: &RenderPlan,
    captured: &EditorialCapture,
    constraint: &EnvelopeConstraint,
    at: ExactRatio,
    output: &mut Vec<AudioBoundaryOrigin>,
    budget: &mut Budget,
) -> Result<(), PlanError> {
    budget.spend(1)?;
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
            let origin = owner.capture(kind, budget)?;
            budget.spend(output.len())?;
            if !output.contains(&origin) {
                output.push(origin);
            }
        }
    }
    Ok(())
}

fn merge_edge(
    edges: &mut Vec<AudioFadeEdge>,
    mut new: AudioFadeEdge,
    budget: &mut Budget,
) -> Result<(), PlanError> {
    budget.spend(edges.len() + 1)?;
    if let Some(old) = edges.iter_mut().find(|edge| edge.at == new.at) {
        if old.editorial {
            new.length = new.length.min(old.length);
        }
        for origin in &old.origins {
            budget.spend(new.origins.len() + origin.instance.repeats.len() + 1)?;
            if !new.origins.contains(origin) {
                new.origins.push(origin.clone());
            }
        }
        *old = new;
    } else {
        edges.push(new);
    }
    Ok(())
}

fn virtual_position(
    position: ExactRatio,
    origin: ExactRatio,
    scale: ExactRatio,
) -> Result<ExactRatio, TimeError> {
    origin.checked_add(position.checked_sub(origin)?.checked_mul(scale)?)
}

/// First output sample whose reference position reaches the next interval.
/// Ceil preserves half-open ownership even for signed or fractional phases.
fn inverse_end(
    start: AudioSample,
    end: AudioSample,
    reference: ExactRatio,
    step: ExactRatio,
    reference_end: AudioSample,
) -> Result<AudioSample, PlanError> {
    let count = ExactRatio::integer(reference_end.0)
        .checked_sub(reference)?
        .checked_div(step)?
        .ceil()?;
    if count <= 0 {
        return Err(PlanError::InvalidPlan(
            "fade reference interval did not advance",
        ));
    }
    // Clamp before converting so a bounded query can inspect a wide domain.
    let remaining = i128::from(end.0) - i128::from(start.0);
    let next = i128::from(start.0)
        .checked_add(count.min(remaining))
        .ok_or(TimeError::Overflow)?;
    Ok(AudioSample(
        i64::try_from(next).map_err(|_| TimeError::Overflow)?,
    ))
}
