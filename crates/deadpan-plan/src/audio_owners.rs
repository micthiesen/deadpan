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
    AudioDefinition, AudioDefinitionSelector, AudioDomain, AudioOwnerRetainedInput,
    AudioQueryLimits, CompiledKind, LookupStats, RenderPlan, SignalSample, SignalTransform,
};
use crate::{AudioBoundaryRule, AudioSampleGrid, AudioSampleMap, PlanError};

#[path = "audio_owner_query_admission.rs"]
pub(super) mod admission;
#[path = "audio_retained_owner_query.rs"]
mod retained_query;
pub use retained_query::AdmittedAudioOwnerQuery;
#[path = "audio_treatment_boundary_admission.rs"]
mod treatment_boundary_admission;
pub use treatment_boundary_admission::AdmittedAudioTreatmentBoundary;
#[path = "audio_owner_query.rs"]
pub(super) mod query_admission;
#[cfg(test)]
use query_admission::{Output, walk_query};
use query_admission::{State, query_tracked};

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

impl<S> crate::audio_allocated::OwnedHeap for AudioOwnerClock<'_, S> {
    fn visit_heap(&self, budget: &mut crate::audio_allocated::HeapBudget) -> Result<(), PlanError> {
        let Self {
            plan: _,
            definition,
            instance,
            kind: _,
            gap_after,
            play: _,
            sampling: _,
            origin,
        } = self;
        budget.visit(definition)?;
        budget.visit(instance)?;
        budget.visit(gap_after)?;
        match origin {
            AudioOwnerClockOrigin::Current => Ok(()),
            AudioOwnerClockOrigin::Retained {
                definition,
                instance,
                kind: _,
                gap_after,
            } => {
                budget.visit(definition)?;
                budget.visit(instance)?;
                budget.visit(gap_after)
            }
        }
    }
}

impl<'plan> AudioOwnerClock<'plan> {
    /// A position between explicitly ordered treatments on this immutable
    /// owner. Zero is after Repeat escalation and before its first treatment;
    /// `order().len()` is after its last treatment. This is an evaluation
    /// boundary, not a new authored effect or a permission to extend allocation.
    pub fn treatment_boundary(
        &self,
        before_stage: usize,
    ) -> Result<AudioTreatmentBoundary<'plan>, PlanError> {
        self.validate_treatment_stage(before_stage)?;
        Ok(AudioTreatmentBoundary {
            owner: self.clone(),
            before_stage,
        })
    }

    fn validate_treatment_stage(&self, before_stage: usize) -> Result<(), PlanError> {
        if before_stage > self.treatments().map_or(0, |value| value.order().len()) {
            return Err(PlanError::InvalidPlan(
                "audio treatment boundary out of range",
            ));
        }
        Ok(())
    }

    /// Same treatment input identity, independent of current Repeat play or
    /// affine sampling. This inspection borrows both clocks without copying.
    pub fn matches_treatment_owner(&self, candidate: &AudioOwnerClock<'_>) -> bool {
        self.belongs_to(candidate.plan)
            && self.definition == candidate.definition
            && self.instance == candidate.instance
            && self.kind == candidate.kind
            && self.gap_after == candidate.gap_after
            && self.origin == candidate.origin
    }

    pub(super) fn same_at(&self, other: &AudioOwnerClock<'_>, at: AudioSample) -> bool {
        self.belongs_to(other.plan)
            && self.definition == other.definition
            && self.instance == other.instance
            && self.kind == other.kind
            && self.gap_after == other.gap_after
            && self.play == other.play
            && self.origin == other.origin
            && self.sampling.local_frames_per_sample() == other.sampling.local_frames_per_sample()
            && matches!((self.sampling.local_at(at), other.sampling.local_at(at)), (Ok(a), Ok(b)) if a == b)
    }

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

/// A checked cut in one owner's serialized treatment program. Matching keeps
/// definition and retained provenance as well as the occurrence. A Repeat's
/// current play and affine clock may change while traversing its whole input.
#[derive(Debug, Clone)]
pub struct AudioTreatmentBoundary<'plan> {
    owner: AudioOwnerClock<'plan>,
    before_stage: usize,
}

impl<'plan> AudioTreatmentBoundary<'plan> {
    pub fn owner(&self) -> &AudioOwnerClock<'plan> {
        &self.owner
    }
    pub fn before_stage(&self) -> usize {
        self.before_stage
    }
    pub fn matches(&self, candidate: &AudioOwnerClock<'_>) -> bool {
        self.owner.matches_treatment_owner(candidate)
    }
    /// Reuse within the SAME checked processing context additionally requires
    /// an equal affine clock. A whole Repeat's current play is evaluated by
    /// each query and is not part of its input identity. Child occurrence IDs
    /// still distinguish plays. This alone cannot identify a raw context.
    pub fn matches_clock_at(&self, candidate: &AudioOwnerClock<'_>, at: AudioSample) -> bool {
        self.matches(candidate)
            && self.owner.sampling.local_frames_per_sample()
                == candidate.sampling.local_frames_per_sample()
            && matches!((self.owner.sampling.local_at(at), candidate.sampling.local_at(at)), (Ok(a), Ok(b)) if a == b)
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
    /// An explicit Tail provider selects the Original return admitted during
    /// its allocated pause. Ancestors must not add other incoming returns.
    /// A default Repeat gap carries the same policy without a separate node.
    pub fn is_explicit_tail_provider(&self) -> bool {
        matches!(
            (
                self.kind,
                &self.plan.nodes[self.plan.by_id[&self.instance.node]].kind
            ),
            (
                AudioOwnerKind::Node,
                CompiledKind::Hold {
                    audio: deadpan_core::HoldAudio::Tail { .. },
                    ..
                }
            ) | (
                AudioOwnerKind::DefaultGap,
                CompiledKind::Repeat {
                    gap_audio: Some(deadpan_core::HoldAudio::Tail { .. }),
                    ..
                }
            )
        )
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
    original_is_silent: bool,
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
    /// The terminal recipe produces zero Original PCM without traversing a
    /// stateful processor. False is inconclusive: Preserve may carry signal
    /// across a silent leaf. This proof grants no silence policy or admission.
    pub fn original_is_silent(&self) -> bool {
        self.original_is_silent
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
    pub fn has_audio_wet_sends(&self) -> bool {
        self.has_audio_wet_sends
    }
    pub fn has_sound_treatments(&self) -> bool {
        self.sounds()
            .values()
            .any(|event| !event.treatments.is_empty())
            || self
                .beat_sounds()
                .values()
                .flat_map(|sounds| sounds.values())
                .any(|event| !event.treatments.is_empty())
    }
    pub fn has_audio_equalizers(&self) -> bool {
        self.has_audio_equalizers
    }

    /// Conservative plan-wide need for contributions through shared effects.
    /// Never select floating-point evaluation order from the requested window.
    pub fn has_audio_component_automation(&self) -> bool {
        self.has_audio_component_automation
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

pub(crate) fn component_automation<'a>(
    recipes: impl Iterator<Item = &'a deadpan_core::AudioTreatments>,
    escalation: bool,
) -> bool {
    let mut varying = escalation;
    let mut shared = false;
    for recipe in recipes {
        shared |= recipe.saturation().is_some()
            || recipe
                .equalizer()
                .is_some_and(|equalizer| !equalizer.is_unity());
        varying |= recipe.clip_gain().is_some_and(|gain| {
            !gain.muted() && (!gain.envelopes().is_empty() || !gain.mute_ranges().is_empty())
        });
    }
    shared && varying
}

impl<'plan> AudioDomain<'plan> {
    /// Treatment evaluation on a captured context, retaining explicit inactive
    /// spans for exhausted bindings instead of fabricating descendant clocks.
    pub fn gain_owners(
        &self,
        samples: Range<AudioSample>,
        limits: AudioQueryLimits,
    ) -> Result<AudioOwnerQuery<'plan>, PlanError> {
        if samples.start < self.samples.start || samples.end > self.samples.end {
            return Err(PlanError::AudioRangeOutOfRange);
        }
        let mut seed = Walk::from_seed(&self.seed);
        seed.allow_inactive = true;
        query(
            self.plan,
            samples.start.0..samples.end.0,
            limits,
            seed,
            AudioSample,
        )
    }

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
        seed.extent = ExactRatio::ZERO..self.exact_duration();
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

impl<'plan> AudioOwnerRetainedInput<'plan> {
    /// Owner clocks on this retained processor's consumer grid, including
    /// hidden history. Exterior ancestors are excluded; the processor and its
    /// current descendants keep their recipes and exact reference timing.
    pub fn owners(
        &self,
        samples: Range<AudioSample>,
        limits: AudioQueryLimits,
    ) -> Result<AudioOwnerQuery<'plan>, PlanError> {
        let mut admission = admission::Unmetered;
        let query = retained_query::query(self, samples, limits, &mut admission)?;
        Ok(query.unmetered())
    }

    /// Query retained owner clocks while charging the actual envelope and
    /// recursive walk to one shared work and allocation admission.
    pub fn owners_admitted<A: crate::AudioSoundContextAdmission>(
        &self,
        samples: Range<AudioSample>,
        limits: AudioQueryLimits,
        admission: &mut A,
    ) -> std::result::Result<AdmittedAudioOwnerQuery<'plan, A::Issuer, A::Lease>, A::Error> {
        retained_query::query(self, samples, limits, admission)
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
    through_processor: bool,
    original_is_silent: bool,
}

fn query<'plan, S: Copy>(
    plan: &'plan RenderPlan,
    samples: Range<i64>,
    limits: AudioQueryLimits,
    seed: Walk,
    label: impl Fn(i64) -> S,
) -> Result<AudioOwnerQuery<'plan, S>, PlanError> {
    query_mapped(plan, samples, limits, seed, label, None)
}

fn query_mapped<'plan, S: Copy>(
    plan: &'plan RenderPlan,
    samples: Range<i64>,
    limits: AudioQueryLimits,
    seed: Walk,
    label: impl Fn(i64) -> S,
    retained: Option<&AudioOwnerRetainedInput<'plan>>,
) -> Result<AudioOwnerQuery<'plan, S>, PlanError> {
    let mut local = Budget {
        remaining: limits.maximum_work,
        lookup: LookupStats::default(),
    };
    let mut policy = admission::Unmetered;
    let mut budget = admission::Budget::new(&mut local, &mut policy);
    let seed = State::plain(seed, ());
    admission::outer::<_, admission::Unmetered>(query_tracked(
        plan,
        samples,
        limits,
        seed,
        label,
        retained,
        &mut budget,
    ))
    .map(|value| value.query.unmetered())
}

#[cfg(test)]
fn walk(
    plan: &RenderPlan,
    state: Walk,
    position: Position,
    output: &mut WalkOutput,
    origin: AudioOwnerClockOrigin,
    budget: &mut Budget,
    depth: usize,
) -> Result<(), PlanError> {
    let mut policy = admission::Unmetered;
    let mut query_budget = admission::Budget::new(budget, &mut policy);
    let old = std::mem::replace(
        output,
        WalkOutput {
            start: 0,
            end: 0,
            owners: Vec::new(),
            support: AudioOwnerSupport::Active,
            through_processor: false,
            original_is_silent: false,
        },
    );
    let mut tracked = Output::plain(old, ());
    let result = admission::outer::<_, admission::Unmetered>(walk_query(
        plan,
        State::plain(state, ()),
        position,
        &mut tracked,
        &origin,
        &mut query_budget,
        depth,
    ));
    *output = tracked.value;
    result
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
pub(in crate::plan) mod tests;

#[cfg(test)]
#[path = "audio_owners/treatment_boundary_admission_tests.rs"]
mod treatment_boundary_admission_tests;
