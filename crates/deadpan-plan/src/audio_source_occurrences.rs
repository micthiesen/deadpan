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

impl RenderPlan {
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
