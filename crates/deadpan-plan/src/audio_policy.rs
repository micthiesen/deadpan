//! Independently evaluate current and retained-placement policy on each grid.
//! Timing records never supply obsolete audio policy or a historical raw body.

use std::ops::Range;

use deadpan_core::{AudioSample, ExactRatio, MAX_DOCUMENT_DEPTH, TimeError};

use super::{
    AudioContent, AudioDomain, AudioQueryLimits, AudioRetimeStage, AudioSignal, AudioSignalContent,
    LookupStats, RenderPlan, SignalSample, SilenceReason,
};
use crate::{AudioBoundaryRule, PlanError};

#[path = "audio_policy_query.rs"]
pub(in crate::plan) mod query;
pub use query::{
    AdmittedAudioPolicyQuery, AdmittedSignalPolicyQuery, AudioSignalPolicyMaskHandoff,
    SignalPolicyMaskRef, SignalPolicyView,
};

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
        self.policy_with_endpoints(samples, limits, true)
    }

    /// Policy applied after canonical Preserve processing. Source placement
    /// endpoints must not erase processed decay; explicit silent Holds remain.
    pub fn policy_after_preserve(
        &self,
        samples: Range<SignalSample>,
        limits: AudioQueryLimits,
    ) -> Result<AudioPolicyQuery<SignalSample>, PlanError> {
        self.policy_with_endpoints(samples, limits, false)
    }

    fn policy_with_endpoints(
        &self,
        samples: Range<SignalSample>,
        limits: AudioQueryLimits,
        source_endpoints: bool,
    ) -> Result<AudioPolicyQuery<SignalSample>, PlanError> {
        self.policy_on_grid(
            samples,
            limits,
            AudioBoundaryRule::PointCeil,
            source_endpoints,
        )
    }

    pub(crate) fn policy_on_grid(
        &self,
        samples: Range<SignalSample>,
        limits: AudioQueryLimits,
        rule: AudioBoundaryRule,
        source_endpoints: bool,
    ) -> Result<AudioPolicyQuery<SignalSample>, PlanError> {
        let mut work = PolicyWork::new(limits)?;
        let suppressed = signal_policy(
            self,
            samples.start.0..samples.end.0,
            rule,
            &mut work,
            0,
            source_endpoints,
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

fn is_selected_endpoint(content: &AudioContent) -> bool {
    matches!(
        content,
        AudioContent::Silence {
            reason: SilenceReason::OutsideSourceSelection
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
        retime
            .pitch
            .processes(retime.child_frames_per_local_frame == ExactRatio::ONE)
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
        if is_selected_endpoint(&content) && endpoint_grid(endpoints, &span.retimes, work)? {
            suppressed.push(span.samples.start.0..span.samples.end.0);
        }
        work.content(content)?;
    }
    if let Some(carrier) = signal.source_voice_hold_carrier() {
        // Sound content keeps its complete independent recipe through input
        // preparation. Only current explicit Hold issuers suppress its output;
        // the carrier's Original source, endpoints and retained bindings do
        // not describe this voice and must not enter its content inventory.
        let holds = carrier.hold_policy_on_grid(range.clone(), work.limits()?, rule)?;
        work.observed(holds.work, holds.lookup)?;
        for hold in holds.rules {
            work.spend(1)?;
            suppressed.push(hold.samples.start.0..hold.samples.end.0);
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
    domain_policy_inner(domain, samples, work, depth, endpoints)
}

fn domain_policy_inner(
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
        if is_selected_endpoint(&span.content) && endpoint_grid(endpoints, &span.retimes, work)? {
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
