//! Exact structural automation clocks after time mapping. This query does not
//! render treatments, alter DSP boundaries, or grant source admission.

use std::ops::Range;

use deadpan_core::{
    AudioSample, ExactRatio, InsertionBias, InstancePath, IterationId, MIX_SAMPLE_RATE,
    RepeatInstance, RetimePurpose, TimeError,
};

use super::audio::{Budget, intersect};
use super::audio_bound::{BindingTarget, BoundPlacement};
use super::audio_domain::{AudioWalkSeed, DomainGap};
use super::{
    AudioDefinition, AudioDefinitionSelector, AudioDomain, AudioQueryLimits, CompiledKind,
    LookupStats, RenderPlan, SignalSample, SignalTransform,
};
use crate::{AudioBoundaryRule, AudioSampleGrid, AudioSampleMap, PlanError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioOwnerKind {
    Node,
    /// The Repeat's default Hold recipe, distinct from its whole-node clock.
    DefaultGap,
}

/// Meaningful retained support of the Original contribution, independent of
/// silence policy or mute. Inactive spans keep their resolved outer owners.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioOwnerSupport {
    Active,
    Inactive,
}

/// The nearest binding that changed this owner's evaluation clock. Ancestors
/// collected before the binding retain their independently evaluated clocks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioOwnerClockOrigin {
    Current,
    Retained {
        definition: Option<AudioDefinitionSelector>,
        instance: InstancePath,
        kind: AudioOwnerKind,
        gap_after: Option<IterationId>,
    },
}

/// A checked owner borrowed from one immutable plan. There is intentionally no
/// public constructor or deserializer for rebinding inspection data to a plan.
#[derive(Debug, Clone)]
pub struct AudioOwnerClock<'plan, S = AudioSample> {
    plan: &'plan RenderPlan,
    definition: Option<AudioDefinitionSelector>,
    instance: InstancePath,
    kind: AudioOwnerKind,
    gap_after: Option<IterationId>,
    /// For a Repeat owner, the 0-based play this span lies in (a gap belongs
    /// to the play before it).
    play: Option<u32>,
    sampling: AudioSampleMap<S>,
    origin: AudioOwnerClockOrigin,
}

impl<'plan> AudioOwnerClock<'plan> {
    /// Record the play this owner's span lies in, for a Repeat owner.
    pub(crate) fn with_play(mut self, play: Option<u32>) -> Self {
        self.play = play;
        self
    }

    pub(in crate::plan) fn current(
        plan: &'plan RenderPlan,
        instance: InstancePath,
        sampling: AudioSampleMap<AudioSample>,
    ) -> Self {
        Self {
            plan,
            definition: None,
            instance,
            kind: AudioOwnerKind::Node,
            gap_after: None,
            play: None,
            sampling,
            origin: AudioOwnerClockOrigin::Current,
        }
    }
}

impl<S: Copy> AudioOwnerClock<'_, S> {
    pub fn belongs_to(&self, plan: &RenderPlan) -> bool {
        std::ptr::eq(self.plan, plan)
    }
    pub fn definition(&self) -> Option<&AudioDefinitionSelector> {
        self.definition.as_ref()
    }
    pub fn instance(&self) -> &InstancePath {
        &self.instance
    }
    pub fn kind(&self) -> AudioOwnerKind {
        self.kind
    }
    pub fn gap_after(&self) -> Option<&IterationId> {
        self.gap_after.as_ref()
    }
    /// Exact nominal owner-local frames at sample boundaries. This is not a
    /// sample-center offset, a percentage, or a claim about stretcher transients.
    pub fn sampling(&self) -> AudioSampleMap<S> {
        self.sampling
    }
    pub fn origin(&self) -> &AudioOwnerClockOrigin {
        &self.origin
    }
    /// The treatment is borrowed from this checked immutable owner. A Repeat
    /// default gap has a clock but no separate BeatNode treatment.
    pub fn treatments(&self) -> Option<&deadpan_core::AudioTreatments> {
        (self.kind == AudioOwnerKind::Node)
            .then(|| &self.plan.nodes[self.plan.by_id[&self.instance.node]].audio_treatments)
    }
    /// A Repeat owner's escalation gain for this span's play, in exact
    /// millidecibels; zero for every other owner.
    pub fn escalation_millidecibels(&self) -> i64 {
        let (AudioOwnerKind::Node, Some(play)) = (self.kind, self.play) else {
            return 0;
        };
        match &self.plan.nodes[self.plan.by_id[&self.instance.node]].kind {
            CompiledKind::Repeat {
                escalation: Some(escalation),
                ..
            } => escalation.gain_millidecibels(play),
            _ => 0,
        }
    }
}

pub struct AudioOwnerSpan<'plan, S = AudioSample> {
    samples: Range<S>,
    owners: Vec<AudioOwnerClock<'plan, S>>,
    support: AudioOwnerSupport,
}

impl<'plan, S: Copy> AudioOwnerSpan<'plan, S> {
    pub fn samples(&self) -> Range<S> {
        self.samples.clone()
    }
    /// Outer to inner. A whole Repeat and its default gap are distinct owners.
    pub fn owners(&self) -> &[AudioOwnerClock<'plan, S>] {
        &self.owners
    }
    pub fn support(&self) -> AudioOwnerSupport {
        self.support
    }
}

pub struct AudioOwnerQuery<'plan, S = AudioSample> {
    plan: &'plan RenderPlan,
    samples: Range<S>,
    spans: Vec<AudioOwnerSpan<'plan, S>>,
    lookup: LookupStats,
    work: usize,
}

impl<'plan, S: Copy> AudioOwnerQuery<'plan, S> {
    pub fn belongs_to(&self, plan: &RenderPlan) -> bool {
        std::ptr::eq(self.plan, plan)
    }
    pub fn samples(&self) -> Range<S> {
        self.samples.clone()
    }
    pub fn spans(&self) -> &[AudioOwnerSpan<'plan, S>] {
        &self.spans
    }
    pub fn lookup(&self) -> LookupStats {
        self.lookup
    }
    pub fn work(&self) -> usize {
        self.work
    }
}

impl RenderPlan {
    pub fn root_audio_treatments(&self) -> &deadpan_core::AudioTreatments {
        &self.nodes[self.root].audio_treatments
    }

    pub fn has_audio_treatments(&self) -> bool {
        self.has_audio_treatments
    }

    /// Gain evaluation may encounter an absent Original contribution beside
    /// independent sounds. Report known empty/exhausted retained support rather
    /// than inventing descendant clocks; other clock failures still reject.
    pub fn audio_gain_owners(
        &self,
        samples: Range<AudioSample>,
        limits: AudioQueryLimits,
    ) -> Result<AudioOwnerQuery<'_>, PlanError> {
        if samples.start.0 < 0 || samples.end > self.audio_duration()? {
            return Err(PlanError::AudioRangeOutOfRange);
        }
        let mut seed = Walk::root(self)?;
        seed.allow_inactive = true;
        query(
            self,
            samples.start.0..samples.end.0,
            limits,
            seed,
            AudioSample,
        )
    }

    /// Resolve structural owner clocks, including nominal traversal through
    /// Preserve, without modifying its continuous DSP input or edge treatment.
    ///
    /// This inspection currently rejects a bound coordinate outside meaningful
    /// retained support (including empty support). No successful partial owner
    /// stack is returned, and callers must not replace that rejection with unity.
    pub fn audio_owners(
        &self,
        samples: Range<AudioSample>,
        limits: AudioQueryLimits,
    ) -> Result<AudioOwnerQuery<'_>, PlanError> {
        if samples.start.0 < 0 || samples.end > self.audio_duration()? {
            return Err(PlanError::AudioRangeOutOfRange);
        }
        let seed = Walk::root(self)?;
        query(
            self,
            samples.start.0..samples.end.0,
            limits,
            seed,
            AudioSample,
        )
    }
}

impl<'plan> AudioDomain<'plan> {
    /// Only this checked physical domain's owners are returned. Captured
    /// exterior ancestors are not silently invented as definition owners.
    pub fn owners(
        &self,
        samples: Range<AudioSample>,
        limits: AudioQueryLimits,
    ) -> Result<AudioOwnerQuery<'plan>, PlanError> {
        if samples.start < self.samples.start || samples.end > self.samples.end {
            return Err(PlanError::AudioRangeOutOfRange);
        }
        query(
            self.plan,
            samples.start.0..samples.end.0,
            limits,
            Walk::from_seed(&self.seed),
            AudioSample,
        )
    }
}

impl<'plan> AudioDefinition<'plan> {
    /// Inspect any authored definition, including whole Sequences and Repeats,
    /// on its intrinsic local-zero PointCeil grid. No outer play is fabricated.
    pub fn owners(
        &self,
        samples: Range<SignalSample>,
        limits: AudioQueryLimits,
    ) -> Result<AudioOwnerQuery<'plan, SignalSample>, PlanError> {
        let mut seed = Walk::root(self.plan)?;
        seed.node = self.root;
        seed.definition = Some(self.selector.clone());
        seed.gap = self.gap.clone();
        seed.extent = ExactRatio::ZERO..ExactRatio::integer(self.duration().frames());
        seed.grid = AudioSampleGrid::new(
            ExactRatio::ZERO,
            seed.grid.frames_per_sample(),
            AudioBoundaryRule::PointCeil,
        )?;
        seed.binding.grid = seed.grid;
        let end = seed.grid.boundary(seed.extent.end)?.0;
        if samples.start.0 < 0 || samples.end.0 > end {
            return Err(PlanError::AudioRangeOutOfRange);
        }
        query(
            self.plan,
            samples.start.0..samples.end.0,
            limits,
            seed,
            SignalSample,
        )
    }
}

#[derive(Clone, Copy)]
struct Placement {
    origin: ExactRatio,
    scale: ExactRatio,
}

impl Placement {
    fn frame(self, local: ExactRatio) -> Result<ExactRatio, TimeError> {
        self.origin.checked_add(local.checked_mul(self.scale)?)
    }
    fn local(self, frame: ExactRatio) -> Result<ExactRatio, TimeError> {
        frame.checked_sub(self.origin)?.checked_div(self.scale)
    }
    fn child(self, start: ExactRatio, scale: ExactRatio) -> Result<Self, TimeError> {
        Ok(Self {
            origin: self.frame(start)?,
            scale: self.scale.checked_mul(scale)?,
        })
    }
}

#[derive(Clone, Copy)]
struct BindingClock {
    grid: AudioSampleGrid<AudioSample>,
    placement: Placement,
}

#[derive(Clone)]
struct Walk {
    node: usize,
    definition: Option<AudioDefinitionSelector>,
    repeats: Vec<RepeatInstance>,
    gap: Option<DomainGap>,
    bypass: Option<BindingTarget>,
    grid: AudioSampleGrid<AudioSample>,
    placement: Placement,
    binding: BindingClock,
    extent: Range<ExactRatio>,
    support: Option<Range<ExactRatio>>,
    allow_inactive: bool,
}

impl Walk {
    fn root(plan: &RenderPlan) -> Result<Self, PlanError> {
        let rate = plan.metadata.presentation_basis.frame_rate;
        let step = ExactRatio::new(
            i128::from(rate.numerator()),
            i128::from(MIX_SAMPLE_RATE) * i128::from(rate.denominator()),
        )?;
        let grid = AudioSampleGrid::new(ExactRatio::ZERO, step, AudioBoundaryRule::RoundEven)?;
        let placement = Placement {
            origin: ExactRatio::ZERO,
            scale: ExactRatio::ONE,
        };
        Ok(Self {
            node: plan.root,
            definition: None,
            repeats: Vec::new(),
            gap: None,
            bypass: None,
            grid,
            placement,
            binding: BindingClock { grid, placement },
            extent: ExactRatio::ZERO..ExactRatio::integer(plan.duration().frames()),
            support: None,
            allow_inactive: false,
        })
    }

    fn from_seed(seed: &AudioWalkSeed) -> Self {
        let placement = Placement {
            origin: seed.transform.project_origin,
            scale: seed.transform.project_frames_per_local_frame,
        };
        Self {
            node: seed.node,
            definition: seed.definition.clone(),
            repeats: seed.repeats.clone(),
            gap: seed.gap.clone(),
            bypass: seed.bypass_binding,
            grid: seed.grid,
            placement,
            binding: BindingClock {
                grid: seed.grid,
                placement,
            },
            extent: seed.extent.clone(),
            support: seed.envelope.clone(),
            allow_inactive: false,
        }
    }

    fn child(&mut self, start: ExactRatio, scale: ExactRatio) -> Result<(), TimeError> {
        self.placement = self.placement.child(start, scale)?;
        self.binding.placement = self.binding.placement.child(start, scale)?;
        Ok(())
    }
}

/// A coordinate and slope anchored at the current query span's first sample.
#[derive(Clone, Copy)]
struct Position {
    at: ExactRatio,
    step: ExactRatio,
}

struct RawOwner {
    definition: Option<AudioDefinitionSelector>,
    instance: InstancePath,
    kind: AudioOwnerKind,
    gap_after: Option<IterationId>,
    play: Option<u32>,
    position: Position,
    origin: AudioOwnerClockOrigin,
}

struct WalkOutput {
    start: i64,
    end: i64,
    owners: Vec<RawOwner>,
    support: AudioOwnerSupport,
}

fn query<'plan, S: Copy>(
    plan: &'plan RenderPlan,
    samples: Range<i64>,
    limits: AudioQueryLimits,
    seed: Walk,
    label: impl Fn(i64) -> S,
) -> Result<AudioOwnerQuery<'plan, S>, PlanError> {
    limits.validate()?;
    if samples.end < samples.start {
        return Err(PlanError::AudioRangeOutOfRange);
    }
    let mut budget = Budget {
        remaining: limits.maximum_work,
        lookup: LookupStats::default(),
    };
    let mut spans = Vec::new();
    let mut cursor = samples.start;
    while cursor < samples.end {
        if spans.len() == limits.maximum_spans {
            return Err(PlanError::AudioQueryLimit("owner span count"));
        }
        let mut output = WalkOutput {
            start: cursor,
            end: samples.end,
            owners: Vec::new(),
            support: AudioOwnerSupport::Active,
        };
        walk(
            plan,
            seed.clone(),
            Position {
                at: ExactRatio::integer(cursor),
                step: ExactRatio::ONE,
            },
            &mut output,
            AudioOwnerClockOrigin::Current,
            &mut budget,
            0,
        )?;
        if output.end <= cursor
            || (output.owners.is_empty() && output.support == AudioOwnerSupport::Active)
        {
            return Err(PlanError::InvalidPlan("owner interval did not advance"));
        }
        let owners = output
            .owners
            .into_iter()
            .map(|owner| {
                Ok(AudioOwnerClock {
                    plan,
                    definition: owner.definition,
                    instance: owner.instance,
                    kind: owner.kind,
                    gap_after: owner.gap_after,
                    play: owner.play,
                    origin: owner.origin,
                    sampling: AudioSampleMap::new(
                        label(cursor),
                        owner.position.at,
                        owner.position.step,
                    )?,
                })
            })
            .collect::<Result<Vec<_>, PlanError>>()?;
        spans.push(AudioOwnerSpan {
            samples: label(cursor)..label(output.end),
            owners,
            support: output.support,
        });
        cursor = output.end;
    }
    Ok(AudioOwnerQuery {
        plan,
        samples: label(samples.start)..label(samples.end),
        spans,
        lookup: budget.lookup,
        work: limits.maximum_work - budget.remaining,
    })
}

fn walk(
    plan: &RenderPlan,
    mut state: Walk,
    position: Position,
    output: &mut WalkOutput,
    origin: AudioOwnerClockOrigin,
    budget: &mut Budget,
    depth: usize,
) -> Result<(), PlanError> {
    if depth > deadpan_core::MAX_DOCUMENT_DEPTH {
        return Err(PlanError::AudioQueryLimit("owner binding depth"));
    }
    let probe_label =
        AudioSample(i64::try_from(position.at.floor()).map_err(|_| TimeError::Overflow)?);
    let (probe, bias) = state.grid.probe(probe_label)?;
    loop {
        budget.spend(1 + state.repeats.len())?;
        budget.lookup.visited_nodes += 1;
        let node = &plan.nodes[state.node];
        let kind = if state.gap.is_some() {
            AudioOwnerKind::DefaultGap
        } else {
            AudioOwnerKind::Node
        };
        let target = if state.gap.is_some() {
            BindingTarget::Gap(state.node)
        } else {
            BindingTarget::Node(state.node)
        };
        let duration = state
            .gap
            .as_ref()
            .map_or(node.inspection.duration, |gap| gap.duration);
        let extent = state.placement.origin
            ..state
                .placement
                .frame(ExactRatio::integer(duration.frames()))?;
        state.extent = intersect(state.extent, extent.clone())?;
        if state.gap.is_some()
            || !matches!(
                node.kind,
                CompiledKind::Sequence { .. }
                    | CompiledKind::Repeat { .. }
                    | CompiledKind::Retime {
                        purpose: RetimePurpose::Partition,
                        ..
                    }
            )
        {
            state.support = Some(match state.support {
                Some(previous) => intersect(previous, extent)?,
                None => extent,
            });
        }
        let allocated =
            state.grid.boundary(state.extent.start)?..state.grid.boundary(state.extent.end)?;
        if !allocated.contains(&probe_label) {
            if state.allow_inactive && matches!(origin, AudioOwnerClockOrigin::Retained { .. }) {
                if probe_label < allocated.start {
                    restrict_end(position, allocated.start.0, output.start, &mut output.end)?;
                }
                output.support = AudioOwnerSupport::Inactive;
                return Ok(());
            }
            return Err(PlanError::InvalidPlan(
                "owner coordinate is outside its allocated interval",
            ));
        }
        restrict_end(position, allocated.end.0, output.start, &mut output.end)?;
        let frame = state
            .grid
            .frame_origin()
            .checked_add(position.at.checked_mul(state.grid.frames_per_sample())?)?;
        let local = Position {
            at: state.placement.local(frame)?,
            step: position
                .step
                .checked_mul(state.grid.frames_per_sample())?
                .checked_div(state.placement.scale)?,
        };
        let instance = InstancePath {
            node: node.inspection.id.clone(),
            repeats: state.repeats.clone(),
        };
        let gap_after = state.gap.as_ref().and_then(|gap| gap.after.clone());
        if state.bypass != Some(target) {
            let transform = SignalTransform {
                signal_origin: state.binding.placement.origin,
                signal_frames_per_local_frame: state.binding.placement.scale,
                grid_origin: state.binding.grid.frame_origin(),
                signal_frames_per_sample: state.binding.grid.frames_per_sample(),
            };
            let binding_frame =
                |value| state.binding.placement.frame(state.placement.local(value)?);
            let allocated_start = state
                .binding
                .grid
                .boundary(binding_frame(state.extent.start)?)?;
            let support = state
                .support
                .as_ref()
                .map(|range| {
                    Ok::<_, TimeError>(binding_frame(range.start)?..binding_frame(range.end)?)
                })
                .transpose()?;
            if let Some(bound) = plan.bound_at(
                state.node,
                &state.repeats,
                state.definition.as_ref(),
                BoundPlacement {
                    transform,
                    grid: state.binding.grid.rebrand(),
                    allocated_start: SignalSample(allocated_start.0),
                    support: support.as_ref(),
                    constraints: &[],
                    gap: state.gap.as_ref(),
                },
                budget,
            )? {
                let Some(domain) = bound.envelope_domain(budget)? else {
                    if state.allow_inactive {
                        output.support = AudioOwnerSupport::Inactive;
                        return Ok(());
                    }
                    return Err(PlanError::InvalidPlan(
                        "owner binding has empty meaningful support",
                    ));
                };
                let physical_at = state
                    .binding
                    .placement
                    .frame(local.at)?
                    .checked_sub(state.binding.grid.frame_origin())?
                    .checked_div(state.binding.grid.frames_per_sample())?;
                let physical_step = local
                    .step
                    .checked_mul(state.binding.placement.scale)?
                    .checked_div(state.binding.grid.frames_per_sample())?;
                let reference = Position {
                    at: bound.reference_at_offset(0)?.checked_add(
                        physical_at
                            .checked_sub(ExactRatio::integer(allocated_start.0))?
                            .checked_mul(bound.reference_samples_per_output_sample())?,
                    )?,
                    step: physical_step.checked_mul(bound.reference_samples_per_output_sample())?,
                };
                let mut retained = Walk::from_seed(&domain.seed);
                retained.allow_inactive = state.allow_inactive;
                return walk(
                    plan,
                    retained,
                    reference,
                    output,
                    AudioOwnerClockOrigin::Retained {
                        definition: state.definition,
                        instance,
                        kind,
                        gap_after,
                    },
                    budget,
                    depth + 1,
                );
            }
        }
        budget.spend(state.definition.is_some() as usize + 1)?;
        output.owners.push(RawOwner {
            definition: state.definition.clone(),
            instance,
            kind,
            gap_after,
            play: None,
            position: local,
            origin: origin.clone(),
        });
        if state.gap.is_some() {
            return Ok(());
        }
        let local_probe = state.placement.local(probe)?;
        match &node.kind {
            CompiledKind::Source { .. } | CompiledKind::Hold { .. } => return Ok(()),
            CompiledKind::Sequence { entries } => {
                let mut left = 0;
                let mut right = entries.len();
                while left < right {
                    budget.spend(1)?;
                    budget.lookup.sequence_comparisons += 1;
                    let middle = left + (right - left) / 2;
                    let comparison = local_probe.compare_integer(entries[middle].end);
                    if comparison.is_gt() || (comparison.is_eq() && bias == InsertionBias::Right) {
                        left = middle + 1;
                    } else {
                        right = middle;
                    }
                }
                let entry = entries
                    .get(left)
                    .ok_or(PlanError::InvalidPlan("owner Sequence has no child"))?;
                state.child(ExactRatio::integer(entry.start), ExactRatio::ONE)?;
                state.node = entry.child;
            }
            CompiledKind::Retime {
                child,
                start,
                scale,
                pitch,
                ..
            } => {
                let inverse = ExactRatio::ONE.checked_div(*scale)?;
                state.child(
                    ExactRatio::ZERO.checked_sub(start.checked_mul(inverse)?)?,
                    inverse,
                )?;
                if pitch.processes(*scale == ExactRatio::ONE) {
                    // Only the binding-resolution clock switches. Automation
                    // remains on the consuming output's exact nominal mapping.
                    // Canonical Preserve prepares its complete selected input;
                    // an outer crop limits delivery, not this hidden support.
                    // Retain `extent` for that delivery allocation separately.
                    let input_end = start
                        .checked_add(scale.checked_mul(ExactRatio::integer(
                            node.inspection.duration.frames(),
                        ))?)?;
                    state.support =
                        Some(state.placement.frame(*start)?..state.placement.frame(input_end)?);
                    state.binding = BindingClock {
                        grid: AudioSampleGrid::new(
                            *start,
                            state.grid.frames_per_sample(),
                            AudioBoundaryRule::PointCeil,
                        )?,
                        placement: Placement {
                            origin: ExactRatio::ZERO,
                            scale: ExactRatio::ONE,
                        },
                    };
                }
                state.node = *child;
            }
            CompiledKind::Repeat { layout, .. } => {
                let location = layout
                    .locate_bounded(local_probe, bias, budget.remaining)
                    .map_err(|error| {
                        if error.code == deadpan_core::DocumentErrorCode::LimitExceeded {
                            PlanError::AudioQueryLimit("owner Repeat work")
                        } else {
                            error.into()
                        }
                    })?;
                budget.spend(location.comparisons)?;
                budget.lookup.iteration_run_comparisons += location.comparisons;
                // The Repeat's own owner was pushed above; record its play.
                if let Some(owner) = output.owners.last_mut() {
                    owner.play = Some(location.play.index);
                }
                let repeat = node.inspection.id.clone();
                if location.in_gap {
                    let begin = location
                        .play
                        .start
                        .checked_add(location.play.duration.frames())
                        .ok_or(TimeError::Overflow)?;
                    state.child(ExactRatio::integer(begin), ExactRatio::ONE)?;
                    if let Some(child) = location.play.gap_child {
                        state.repeats.push(RepeatInstance {
                            node: repeat,
                            iteration: location.play.iteration,
                        });
                        state.node = plan.by_id[&child];
                    } else {
                        state.gap = Some(DomainGap {
                            after: Some(location.play.iteration),
                            duration: location.play.gap_after,
                        });
                    }
                } else {
                    state.child(ExactRatio::integer(location.play.start), ExactRatio::ONE)?;
                    state.repeats.push(RepeatInstance {
                        node: repeat,
                        iteration: location.play.iteration,
                    });
                    state.node = plan.by_id[&location.play.child];
                }
            }
        }
    }
}

fn restrict_end(
    position: Position,
    boundary: i64,
    start: i64,
    end: &mut i64,
) -> Result<(), PlanError> {
    let count = ExactRatio::integer(boundary)
        .checked_sub(position.at)?
        .checked_div(position.step)?
        .ceil()?;
    if count <= 0 {
        return Err(PlanError::InvalidPlan("owner boundary did not advance"));
    }
    let candidate = i128::from(start)
        .checked_add(count)
        .ok_or(TimeError::Overflow)?;
    if candidate < i128::from(*end) {
        *end = i64::try_from(candidate).map_err(|_| TimeError::Overflow)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "audio_owners/tests.rs"]
mod tests;
