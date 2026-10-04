//! Retained independent processing layouts share one preparation controller.

use super::*;
use deadpan_core::{
    ExactFrameRange, FrozenAudioLayout, SoundRippleMap, SoundRippleNode, SoundRoute,
};
use deadpan_plan::{
    AudioBoundaryRule, AudioRoutedRoot, AudioSampleGrid, AudioSoundRoute, AudioSourceOccurrence,
    AudioSourceVoiceRecipe,
};

pub(super) type SoundProcessingPlans = BTreeMap<AudioTimingId, Arc<RenderPlan>>;

impl StageAudio {
    pub(super) fn prepare_sound_processing_plans(
        &mut self,
        plan: &RenderPlan,
        control: WorkControl<'_>,
    ) -> Result<SoundProcessingPlans, StageAudioError> {
        // Only the first layout owns processing. Intermediate clocks translate
        // that output; compiling them would duplicate graphs and asset records.
        // There are at most MAX_DOCUMENT_SOUNDS first clocks, and their selected
        // asset counts together cannot exceed the same authored event bound.
        let mut definitions: BTreeMap<_, (&FrozenAudioLayout, BTreeSet<AssetId>)> = BTreeMap::new();
        for (owner, events) in plan.beat_sounds() {
            for (sound, event) in events {
                let clocks = plan.beat_sound_clock_layouts(owner, sound)?;
                control.spend_plan_work(clocks.len().max(1))?;
                if let Some((clock, layout)) = clocks.first() {
                    definitions
                        .entry((*clock).clone())
                        .or_insert_with(|| (*layout, BTreeSet::new()))
                        .1
                        .insert(event.source.asset.clone());
                }
            }
        }
        let mut prepared = BTreeMap::new();
        for (clock, (layout, assets)) in definitions {
            if !self.sound_processing_plans.contains_key(&clock) {
                control
                    .spend_plan_work(RenderPlan::sound_processing_layout_work(layout, &assets)?)?;
                let retained = plan.compile_sound_processing_layout(layout, &assets)?;
                control.check()?;
                self.sound_processing_plans
                    .insert(clock.clone(), Arc::new(retained));
            }
            prepared.insert(
                clock.clone(),
                Arc::clone(&self.sound_processing_plans[&clock]),
            );
        }
        Ok(prepared)
    }
}

/// Every placement contributes one rounded allocation. Keeping the intermediate
/// grids prevents a later move from reviving samples clipped by an earlier move.
pub(super) fn route_occurrence<'plan>(
    original: AudioSourceOccurrence<'plan>,
    placements: &[Range<ExactRatio>],
    control: WorkControl<'_>,
) -> Result<AudioRoutedRoot<'plan>, StageAudioError> {
    let extent = original.extent();
    if placements.len() < 2 || placements.first() != Some(&extent) {
        return Err(PlanError::InvalidPlan("sound clock placements are incomplete").into());
    }
    let length = extent.end.checked_sub(extent.start)?;
    let mut route = SoundRoute::identity(length).map_err(PlanError::from)?;
    let rate = original.plan().metadata().presentation_basis.frame_rate;
    let step = ExactRatio::new(
        i128::from(rate.numerator()),
        48_000 * i128::from(rate.denominator()),
    )?;
    let mut grids = Vec::with_capacity(placements.len());
    for (index, placement) in placements.iter().enumerate() {
        // Ripple admission examines its accumulated graph, so charge the
        // retained prefix as well as the newly appended operation.
        control.spend_plan_work(index + 1)?;
        if placement.end.checked_sub(placement.start)? != length {
            return Err(PlanError::InvalidPlan("sound clock changed processing extent").into());
        }
        grids.push(AudioSampleGrid::new(
            ExactRatio::ZERO.checked_sub(placement.start)?,
            step,
            AudioBoundaryRule::RoundEven,
        )?);
        if index != 0 {
            let keep = SoundRippleMap::new(
                length,
                0,
                vec![SoundRippleNode::Keep {
                    range: ExactFrameRange {
                        start: ExactRatio::ZERO,
                        end: length,
                    },
                }],
            )
            .map_err(PlanError::from)?;
            route = route.ripple(keep).map_err(PlanError::from)?;
        }
    }
    Ok(AudioRoutedRoot::occurrence(
        original,
        AudioSoundRoute::<AudioSample>::new(route, grids)?,
    )?)
}

pub(super) fn placements(
    original: &AudioSourceOccurrence<'_>,
    current: &AudioSourceOccurrence<'_>,
    clocks: &[deadpan_plan::AudioSoundClockScope<'_>],
    control: WorkControl<'_>,
) -> Result<Vec<Range<ExactRatio>>, StageAudioError> {
    let extent = original.extent();
    let mut placements = Vec::with_capacity(clocks.len() + 1);
    let mut first_origin = None;
    for clock in clocks {
        let (historical_instance, remap_work) = clock
            .remap_instance_with_work(current.instance(), control.query_limits()?.maximum_work)?;
        control.spend_plan_work(remap_work)?;
        let projection = clock
            .historical_layout()
            .project(
                &historical_instance,
                ExactRatio::ZERO,
                None,
                control
                    .query_limits()?
                    .maximum_work
                    .min(deadpan_core::MAX_DOCUMENT_NODES),
            )
            .map_err(PlanError::from)?;
        control.spend_plan_work(projection.work)?;
        let first = *first_origin.get_or_insert(projection.origin);
        let shift = projection.origin.checked_sub(first)?;
        // Core's paired proof establishes that each frozen owner path has the
        // same nested processing graph and stable Repeat labels.
        placements.push(extent.start.checked_add(shift)?..extent.end.checked_add(shift)?);
    }
    placements.push(current.extent());
    Ok(placements)
}

#[cfg(test)]
#[path = "sound_clocks/tests.rs"]
mod tests;

pub(super) fn historical_occurrence<'plan>(
    plan: &'plan RenderPlan,
    event: &deadpan_core::BeatSound,
    instance: deadpan_core::InstancePath,
    control: WorkControl<'_>,
) -> Result<AudioSourceOccurrence<'plan>, StageAudioError> {
    let voice = plan.source_voice_occurrence(
        instance,
        AudioSourceVoiceRecipe {
            source: event.source.clone(),
            mapping: event.mapping,
            offset: event.offset,
        },
        control.query_limits()?,
    )?;
    control.spend_plan_work(voice.construction_work())?;
    Ok(voice)
}
