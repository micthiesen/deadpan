//! A bounded window of independent concrete voices, retained across PCM reads.

use super::*;
use deadpan_core::SourceAudio;

/// Current occurrences of one independent recipe that can contribute to a
/// checked root window. The handles retain their complete processing graphs, so
/// reading this window in smaller blocks does not create new DSP identities.
/// This is preparation state, not persisted sound ownership or source admission.
#[derive(Debug, Clone)]
pub struct AudioSourceOccurrences<'plan> {
    plan: &'plan RenderPlan,
    owner: NodeId,
    source: SourceAudio,
    samples: Range<AudioSample>,
    voices: Vec<AudioSourceOccurrence<'plan>>,
    work: usize,
}

/// Planning-only paths and exact processing allocations. These do not grant
/// source access or replace the full voice constructor before PCM preparation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioSourceOccurrenceSupport {
    instance: InstancePath,
    samples: Range<AudioSample>,
}

impl AudioSourceOccurrenceSupport {
    pub fn instance(&self) -> &InstancePath {
        &self.instance
    }
    pub fn samples(&self) -> Range<AudioSample> {
        self.samples.clone()
    }
}

#[derive(Debug, Clone)]
pub struct AudioSourceOccurrenceSupports {
    supports: Vec<AudioSourceOccurrenceSupport>,
    work: usize,
}

impl AudioSourceOccurrenceSupports {
    pub fn supports(&self) -> &[AudioSourceOccurrenceSupport] {
        &self.supports
    }
    pub fn construction_work(&self) -> usize {
        self.work
    }
}

impl crate::audio_allocated::OwnedHeap for AudioSourceOccurrences<'_> {
    fn visit_heap(&self, budget: &mut crate::audio_allocated::HeapBudget) -> Result<(), PlanError> {
        let Self {
            plan: _,
            owner,
            source,
            samples: _,
            voices,
            work: _,
        } = self;
        budget.visit(owner)?;
        budget.visit(source)?;
        budget.visit(voices)
    }
}

impl RenderPlan {
    /// Discover exact current paths without allocating ordinary voices. Cases
    /// with enclosing Preserve processing retain full constructor validation.
    /// Full voice preparation remains mandatory before reading any media.
    pub fn source_voice_occurrence_supports(
        &self,
        owner: &NodeId,
        recipe: AudioSourceVoiceRecipe,
        samples: Range<AudioSample>,
        limits: AudioQueryLimits,
    ) -> Result<AudioSourceOccurrenceSupports, PlanError> {
        collect_supports(self, owner, recipe, samples, limits, None)
    }

    /// Collect only current occurrences whose processing allocation overlaps
    /// `samples`. Preserve history may make this larger than visible placement.
    /// Work and aggregate retained runs are bounded across the entire batch;
    /// limits never silently drop a voice. An empty batch still validates the
    /// complete source recipe, and its reader must admit that dependency.
    pub fn source_voice_occurrences(
        &self,
        owner: &NodeId,
        recipe: AudioSourceVoiceRecipe,
        samples: Range<AudioSample>,
        limits: AudioQueryLimits,
    ) -> Result<AudioSourceOccurrences<'_>, PlanError> {
        collect(self, owner, recipe, samples, limits, None)
    }
}

impl<'plan> AudioSoundOwnerContext<'plan> {
    /// Exact planning-only discovery within this checked processing context.
    pub fn source_voice_occurrence_supports(
        &self,
        owner: &NodeId,
        recipe: AudioSourceVoiceRecipe,
        samples: Range<AudioSample>,
        limits: AudioQueryLimits,
    ) -> Result<AudioSourceOccurrenceSupports, PlanError> {
        collect_supports(self.plan, owner, recipe, samples, limits, Some(self))
    }

    /// One checked occurrence within this owner, without enumerating its
    /// siblings. Used when a saved sound clock has already proved its alias.
    pub fn source_voice_occurrence(
        &self,
        instance: InstancePath,
        recipe: AudioSourceVoiceRecipe,
        limits: AudioQueryLimits,
    ) -> Result<AudioSourceOccurrence<'plan>, PlanError> {
        self.plan
            .source_voice_occurrence_in_context(instance, recipe, limits, Some(self))
    }

    /// Independent current occurrences feeding this checked owner, including
    /// history hidden by exterior crops. Missing/out-of-scope voices still
    /// validate their source recipe; no dependency or permission is waived.
    pub fn source_voice_occurrences(
        &self,
        owner: &NodeId,
        recipe: AudioSourceVoiceRecipe,
        samples: Range<AudioSample>,
        limits: AudioQueryLimits,
    ) -> Result<AudioSourceOccurrences<'plan>, PlanError> {
        collect(self.plan, owner, recipe, samples, limits, Some(self))
    }
}

fn collect_supports(
    plan: &RenderPlan,
    owner: &NodeId,
    recipe: AudioSourceVoiceRecipe,
    samples: Range<AudioSample>,
    limits: AudioQueryLimits,
    context: Option<&AudioSoundOwnerContext<'_>>,
) -> Result<AudioSourceOccurrenceSupports, PlanError> {
    limits.validate()?;
    source_occurrence::checked_owner_recipe(plan, owner, &recipe)?;
    let query = plan.audio_owner_occurrences_in_context(owner, samples.clone(), limits, context)?;
    let mut work = query.work();
    let mut runs = query.occurrences().len();
    let mut supports = Vec::with_capacity(runs);
    for occurrence in query.occurrences() {
        let maximum_work = limits
            .maximum_work
            .checked_sub(work)
            .filter(|remaining| *remaining > 0)
            .ok_or(PlanError::AudioQueryLimit("source occurrence batch work"))?;
        let maximum_spans = limits
            .maximum_spans
            .checked_sub(runs)
            .filter(|remaining| *remaining > 0)
            .ok_or(PlanError::AudioQueryLimit("source occurrence batch runs"))?;
        let found = source_occurrence::support::discover(
            plan,
            occurrence.instance(),
            &recipe,
            AudioQueryLimits {
                maximum_work,
                maximum_spans,
            },
            context,
        )?;
        work = work
            .checked_add(found.work)
            .ok_or(PlanError::AudioQueryLimit("source occurrence batch work"))?;
        runs = runs
            .checked_add(found.runs)
            .ok_or(PlanError::AudioQueryLimit("source occurrence batch runs"))?;
        if found.samples.start < samples.end && samples.start < found.samples.end {
            supports.push(AudioSourceOccurrenceSupport {
                instance: occurrence.instance().clone(),
                samples: found.samples,
            });
        }
    }
    Ok(AudioSourceOccurrenceSupports { supports, work })
}

fn collect<'plan>(
    plan: &'plan RenderPlan,
    owner: &NodeId,
    recipe: AudioSourceVoiceRecipe,
    samples: Range<AudioSample>,
    limits: AudioQueryLimits,
    context: Option<&AudioSoundOwnerContext<'plan>>,
) -> Result<AudioSourceOccurrences<'plan>, PlanError> {
    limits.validate()?;
    let checked = source_occurrence::checked_owner_voice(plan, owner, recipe.clone())?;
    let query = plan.audio_owner_occurrences_in_context(owner, samples.clone(), limits, context)?;
    let mut work = query.work();
    let mut runs = query.occurrences().len();
    let mut voices = Vec::with_capacity(runs);
    for occurrence in query.occurrences() {
        let remaining_work = limits
            .maximum_work
            .checked_sub(work)
            .filter(|remaining| *remaining > 0)
            .ok_or(PlanError::AudioQueryLimit("source occurrence batch work"))?;
        let remaining_runs = limits
            .maximum_spans
            .checked_sub(runs)
            .filter(|remaining| *remaining > 0)
            .ok_or(PlanError::AudioQueryLimit("source occurrence batch runs"))?;
        let voice = plan.source_voice_occurrence_in_context(
            occurrence.instance().clone(),
            recipe.clone(),
            AudioQueryLimits {
                maximum_work: remaining_work,
                maximum_spans: remaining_runs,
            },
            context,
        )?;
        work = work
            .checked_add(voice.construction_work())
            .ok_or(PlanError::AudioQueryLimit("source occurrence batch work"))?;
        runs = runs
            .checked_add(voice.retained_runs())
            .ok_or(PlanError::AudioQueryLimit("source occurrence batch runs"))?;
        let extent = voice.samples();
        if extent.start < samples.end && samples.start < extent.end {
            voices.push(voice);
        }
    }
    Ok(AudioSourceOccurrences {
        plan,
        owner: owner.clone(),
        source: checked.source().clone(),
        samples,
        voices,
        work,
    })
}

impl<'plan> AudioSourceOccurrences<'plan> {
    pub fn belongs_to(&self, plan: &RenderPlan) -> bool {
        std::ptr::eq(self.plan, plan)
    }
    pub fn owner(&self) -> &NodeId {
        &self.owner
    }
    pub fn source(&self) -> &SourceAudio {
        &self.source
    }
    pub fn samples(&self) -> Range<AudioSample> {
        self.samples.clone()
    }
    pub fn voices(&self) -> &[AudioSourceOccurrence<'plan>] {
        &self.voices
    }
    pub fn construction_work(&self) -> usize {
        self.work
    }
}
