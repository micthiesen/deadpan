//! Independently evaluate current and retained-placement policy on each grid.
//! Timing records never supply obsolete audio policy or a historical raw body.

use std::ops::Range;

use deadpan_core::{AudioSample, ExactRatio, MAX_DOCUMENT_DEPTH, PitchPolicy, TimeError};

use super::{
    AudioBound, AudioBoundDomain, AudioContent, AudioDomain, AudioQueryLimits, AudioRetimeStage,
    AudioSignal, AudioSignalContent, LookupStats, RenderPlan, SignalSample, SilenceReason,
};
use crate::{AudioBoundaryRule, PlanError};

#[derive(Debug, Clone)]
pub struct AudioPolicyQuery<S> {
    pub suppressed: Vec<Range<S>>,
    /// Current owned content encountered on either policy path. The audio host
    /// checks unsupported treatments before any source I/O or PCM mutation.
    pub contents: Vec<AudioContent>,
    pub lookup: LookupStats,
    pub work: usize,
}

struct PolicyWork {
    remaining: usize,
    maximum: usize,
    maximum_spans: usize,
    contents: Vec<AudioContent>,
    lookup: LookupStats,
}

impl PolicyWork {
    fn new(limits: AudioQueryLimits) -> Result<Self, PlanError> {
        limits.validate()?;
        Ok(Self {
            remaining: limits.maximum_work,
            maximum: limits.maximum_work,
            maximum_spans: limits.maximum_spans,
            contents: Vec::new(),
            lookup: LookupStats::default(),
        })
    }
    fn spend(&mut self, work: usize) -> Result<(), PlanError> {
        self.remaining = self
            .remaining
            .checked_sub(work)
            .ok_or(PlanError::AudioQueryLimit("recursive policy work"))?;
        Ok(())
    }
    fn limits(&self) -> Result<AudioQueryLimits, PlanError> {
        if self.remaining == 0 {
            return Err(PlanError::AudioQueryLimit("recursive policy work"));
        }
        Ok(AudioQueryLimits {
            maximum_spans: self.maximum_spans,
            maximum_work: self.remaining,
        })
    }
    fn observed(&mut self, work: usize, lookup: LookupStats) -> Result<(), PlanError> {
        self.spend(work)?;
        self.lookup.visited_nodes += lookup.visited_nodes;
        self.lookup.sequence_comparisons += lookup.sequence_comparisons;
        self.lookup.iteration_run_comparisons += lookup.iteration_run_comparisons;
        Ok(())
    }
    fn content(&mut self, content: AudioContent) -> Result<(), PlanError> {
        // This inventory covers several policy grids, not one output
        // partition. Bound its aggregate storage by the shared work allowance;
        // reusing maximum_spans here rejects dense valid bound repetitions
        // merely because both their current and retained recipes are checked.
        self.spend(1)?;
        self.contents.push(content);
        Ok(())
    }
    fn enter(&mut self, depth: usize) -> Result<(), PlanError> {
        if depth > MAX_DOCUMENT_DEPTH {
            return Err(PlanError::AudioQueryLimit("policy depth"));
        }
        self.spend(1)
    }
    fn result<S>(
        self,
        mut suppressed: Vec<Range<i64>>,
        label: fn(i64) -> S,
    ) -> AudioPolicyQuery<S> {
        suppressed.sort_unstable_by_key(|range| range.start);
        let mut merged: Vec<Range<i64>> = Vec::new();
        for range in suppressed {
            if range.start >= range.end {
                continue;
            }
            if let Some(last) = merged.last_mut()
                && range.start <= last.end
            {
                last.end = last.end.max(range.end);
            } else {
                merged.push(range);
            }
        }
        AudioPolicyQuery {
            suppressed: merged
                .into_iter()
                .map(|r| label(r.start)..label(r.end))
                .collect(),
            contents: self.contents,
            lookup: self.lookup,
            work: self.maximum - self.remaining,
        }
    }
}

impl RenderPlan {
    pub fn audio_policy(
        &self,
        samples: Range<AudioSample>,
        limits: AudioQueryLimits,
    ) -> Result<AudioPolicyQuery<AudioSample>, PlanError> {
        let mut work = PolicyWork::new(limits)?;
        let suppressed = signal_policy(
            &self.audio_signal(),
            samples.start.0..samples.end.0,
            AudioBoundaryRule::RoundEven,
            &mut work,
            0,
            true,
        )?;
        Ok(work.result(suppressed, AudioSample))
    }
}

impl AudioSignal<'_> {
    pub fn policy(
        &self,
        samples: Range<SignalSample>,
        limits: AudioQueryLimits,
    ) -> Result<AudioPolicyQuery<SignalSample>, PlanError> {
        let mut work = PolicyWork::new(limits)?;
        let suppressed = signal_policy(
            self,
            samples.start.0..samples.end.0,
            AudioBoundaryRule::PointCeil,
            &mut work,
            0,
            true,
        )?;
        Ok(work.result(suppressed, SignalSample))
    }
}

impl AudioDomain<'_> {
    pub fn policy(
        &self,
        samples: Range<AudioSample>,
        limits: AudioQueryLimits,
    ) -> Result<AudioPolicyQuery<AudioSample>, PlanError> {
        let mut work = PolicyWork::new(limits)?;
        let suppressed = domain_policy(self, samples.start.0..samples.end.0, &mut work, 0, true)?;
        Ok(work.result(suppressed, AudioSample))
    }
}

fn is_suppressed(content: &AudioContent) -> bool {
    matches!(
        content,
        AudioContent::Silence {
            reason: SilenceReason::SilentHold
        }
    )
}

// Endpoint masks belong to the physical sampling grid. A downstream Preserve
// prepares those masked inputs, but its decay is not an authored SilentHold.
// Explicit Hold policies still cross every stage and are queried on its output.
fn endpoint_grid(
    enabled: bool,
    retimes: &[AudioRetimeStage],
    work: &mut PolicyWork,
) -> Result<bool, PlanError> {
    if !enabled {
        return Ok(false);
    }
    work.spend(retimes.len())?;
    Ok(!retimes.iter().any(|retime| {
        retime.pitch == PitchPolicy::Preserve
            && retime.child_frames_per_local_frame != ExactRatio::ONE
    }))
}

fn signal_policy(
    signal: &AudioSignal<'_>,
    samples: Range<i64>,
    rule: AudioBoundaryRule,
    work: &mut PolicyWork,
    depth: usize,
    endpoints: bool,
) -> Result<Vec<Range<i64>>, PlanError> {
    work.enter(depth)?;
    let range = SignalSample(samples.start)..SignalSample(samples.end);
    let current = signal.query_inner(range.clone(), work.limits()?, rule, false, false)?;
    work.observed(current.work, current.lookup)?;
    let mut suppressed = Vec::new();
    for span in current.spans {
        let AudioSignalContent::Leaf(content) = span.content else {
            return Err(PlanError::InvalidPlan("current policy retained processing"));
        };
        if is_suppressed(&content) {
            suppressed.push(span.samples.start.0..span.samples.end.0);
        }
        work.content(content)?;
    }
    if !signal.has_audio_bindings() || samples.is_empty() {
        return Ok(suppressed);
    }
    let bindings = signal.query_inner(range, work.limits()?, rule, false, true)?;
    work.observed(bindings.work, bindings.lookup)?;
    for span in bindings.spans {
        if let AudioSignalContent::Bound(bound) = span.content {
            let endpoints = endpoint_grid(endpoints, &span.retimes, work)?;
            suppressed.extend(bound_policy(
                &bound,
                span.samples.start.0..span.samples.end.0,
                span.allocated_samples.start.0,
                work,
                depth + 1,
                endpoints,
            )?);
        }
    }
    Ok(suppressed)
}

fn domain_policy(
    domain: &AudioDomain<'_>,
    samples: Range<i64>,
    work: &mut PolicyWork,
    depth: usize,
    endpoints: bool,
) -> Result<Vec<Range<i64>>, PlanError> {
    work.enter(depth)?;
    let range = AudioSample(samples.start)..AudioSample(samples.end);
    let current = domain.audio(range.clone(), work.limits()?)?;
    work.observed(current.work, current.lookup)?;
    let mut suppressed = Vec::new();
    for span in current.spans {
        if is_suppressed(&span.content) {
            suppressed.push(span.samples.start.0..span.samples.end.0);
        }
        // These are the retained raw operand's meaningful edges, independently
        // queried on its own absolute grid, never inferred from zero PCM.
        if endpoint_grid(endpoints, &span.retimes, work)? {
            add_outside(
                &mut suppressed,
                span.samples.start.0..span.samples.end.0,
                span.envelope_samples.start.0..span.envelope_samples.end.0,
            );
        }
        work.content(span.content)?;
    }
    if !domain.has_audio_bindings() || samples.is_empty() {
        return Ok(suppressed);
    }
    let bindings = domain.processing_inner(range, work.limits()?, false)?;
    work.observed(bindings.work, bindings.lookup)?;
    for span in bindings.spans {
        if let AudioSignalContent::Bound(bound) = span.content {
            let endpoints = endpoint_grid(endpoints, &span.retimes, work)?;
            suppressed.extend(bound_policy(
                &bound,
                span.samples.start.0..span.samples.end.0,
                span.allocated_samples.start.0,
                work,
                depth + 1,
                endpoints,
            )?);
        }
    }
    Ok(suppressed)
}

fn add_outside(out: &mut Vec<Range<i64>>, samples: Range<i64>, support: Range<i64>) {
    if samples.start < support.start {
        out.push(samples.start..samples.end.min(support.start));
    }
    if samples.end > support.end {
        out.push(samples.start.max(support.end)..samples.end);
    }
}

fn bound_policy(
    bound: &AudioBound<'_>,
    samples: Range<i64>,
    anchor: i64,
    work: &mut PolicyWork,
    depth: usize,
    endpoints: bool,
) -> Result<Vec<Range<i64>>, PlanError> {
    work.enter(depth)?;
    let raw = bound.raw_domain()?;
    let support = match &raw {
        AudioBoundDomain::Root(domain) => {
            let range = domain.root_samples();
            range.start.0..range.end.0
        }
        AudioBoundDomain::Point(domain) => {
            let range = domain.reference_samples();
            range.start.0..range.end.0
        }
        AudioBoundDomain::Empty => return Ok(if endpoints { vec![samples] } else { Vec::new() }),
    };
    if support.is_empty() {
        return Ok(if endpoints { vec![samples] } else { Vec::new() });
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
    let mut suppressed = Vec::new();
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
        );
    }
    if clipped_start >= clipped_end {
        return Ok(suppressed);
    }
    let selection = i64::try_from(clipped_start).map_err(|_| TimeError::Overflow)?
        ..i64::try_from(clipped_end).map_err(|_| TimeError::Overflow)?;
    let retained = match raw {
        AudioBoundDomain::Root(domain) => {
            domain_policy(&domain, selection, work, depth + 1, endpoints)?
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
            signal_policy(
                &domain.signal(),
                range,
                AudioBoundaryRule::PointCeil,
                work,
                depth + 1,
                endpoints,
            )?
            .into_iter()
            .map(|range| {
                Ok(range.start.checked_add(origin).ok_or(TimeError::Overflow)?
                    ..range.end.checked_add(origin).ok_or(TimeError::Overflow)?)
            })
            .collect::<Result<Vec<_>, TimeError>>()?
        }
        AudioBoundDomain::Empty => unreachable!("empty operand returned before selection"),
    };
    for range in retained {
        work.spend(1)?;
        let mapped = inverse(range.start)?..inverse(range.end)?;
        if !mapped.is_empty() {
            suppressed.push(mapped);
        }
    }
    Ok(suppressed)
}
