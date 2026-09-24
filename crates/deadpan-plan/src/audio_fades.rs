//! Creative edge envelopes on the consuming output clock. Raw endpoint and
//! explicit-silence policies remain the responsibility of the audio readers.

use std::ops::Range;

use deadpan_core::{AudioSample, ExactRatio, TimeError};
use serde::Serialize;

use super::audio::{AudioWalkSpan, Budget};
use super::audio_domain::AudioWalkSeed;
use super::{
    AudioBoundaries, AudioDomain, AudioProcessingSpan, AudioQueryLimits, AudioSignalContent,
    AudioSpan, LookupStats, PlanError, RenderPlan,
};

/// One creative envelope evaluated after mapping to the final output. Progress
/// advances by exactly one per output sample, including when its initial phase
/// is fractional. Lengths below two have no automatic creative fade.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AudioFadeSpan {
    pub samples: Range<AudioSample>,
    pub length: u64,
    pub progress_at_start: ExactRatio,
    pub boundaries: AudioBoundaries,
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
        let span = match plan.audio_walk(cursor, &mut budget, seed, true, true, None)? {
            AudioWalkSpan::Leaf(leaf) => ordinary(leaf, cursor, samples.end)?,
            AudioWalkSpan::Stage(processing) => match &processing.content {
                AudioSignalContent::Bound(_) => {
                    bound_fade(processing, cursor, samples.end, &mut budget)?
                }
                AudioSignalContent::Stage(_) => {
                    // The first opaque output owns creative fades. Flatten its
                    // current geometry in this output grid, retaining ancestor
                    // constraints and deliberately ignoring input bindings.
                    let AudioWalkSpan::Leaf(leaf) =
                        plan.audio_walk(cursor, &mut budget, seed, false, false, None)?
                    else {
                        return Err(PlanError::InvalidPlan("flattened fade retained a stage"));
                    };
                    ordinary(
                        leaf,
                        cursor,
                        samples.end.min(processing.allocated_samples.end),
                    )?
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
    Ok(AudioFadeSpan {
        samples: start..end.min(leaf.allocated_samples.end),
        length: leaf.envelope.length(),
        progress_at_start: ExactRatio::new(leaf.envelope.progress_at(start)?, 1)?,
        boundaries: leaf.boundaries,
    })
}

fn bound_fade(
    processing: AudioProcessingSpan<'_>,
    start: AudioSample,
    end: AudioSample,
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
    let Some(domain) = bound.envelope_domain(budget)? else {
        return Ok(empty(start..end));
    };
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
    let AudioWalkSpan::Leaf(leaf) =
        domain
            .plan
            .audio_walk(probe, budget, Some(&domain.seed), false, false, None)?
    else {
        return Err(PlanError::InvalidPlan("bound fade retained a stage"));
    };
    let end = inverse_end(start, end, q, step, leaf.allocated_samples.end)?;
    let origin = bound.lattice_origin();
    let scale = bound.envelope_scale();
    if scale.compare_integer(0).is_le() {
        return Err(PlanError::InvalidPlan(
            "fade virtual scale must be positive",
        ));
    }
    let virtual_start = virtual_position(leaf.envelope_extent.start, origin, scale)?;
    let virtual_end = virtual_position(leaf.envelope_extent.end, origin, scale)?;
    let first = leaf.grid.boundary(virtual_start)?;
    let last = leaf.grid.boundary(virtual_end)?;
    let length =
        u64::try_from(i128::from(last.0) - i128::from(first.0)).map_err(|_| TimeError::Overflow)?;
    Ok(AudioFadeSpan {
        samples: start..end,
        length,
        progress_at_start: q
            .checked_sub(ExactRatio::integer(leaf.envelope_samples.start.0))?
            .checked_div(step)?,
        boundaries: leaf.boundaries,
    })
}

fn empty(samples: Range<AudioSample>) -> AudioFadeSpan {
    AudioFadeSpan {
        samples,
        length: 0,
        progress_at_start: ExactRatio::ZERO,
        boundaries: AudioBoundaries::default(),
    }
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
