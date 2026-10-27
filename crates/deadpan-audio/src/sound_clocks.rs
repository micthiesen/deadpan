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
        samples: Range<AudioSample>,
        control: WorkControl<'_>,
    ) -> Result<SoundProcessingPlans, StageAudioError> {
        self.prepare_sound_processing_plans_in_context(plan, samples, control, None)
    }

    pub(super) fn prepare_sound_processing_plans_in_context(
        &mut self,
        plan: &RenderPlan,
        samples: Range<AudioSample>,
        control: WorkControl<'_>,
        context: Option<&deadpan_plan::AudioSoundOwnerContext<'_>>,
    ) -> Result<SoundProcessingPlans, StageAudioError> {
        // Births can make a different reference first for different occurrences.
        // Compile only first clocks needed by this bounded query. Asset sets are
        // complete across the event inventory so a cached plan remains valid on
        // a later query that encounters another event sharing the same clock.
        let mut definitions: BTreeMap<_, (&FrozenAudioLayout, BTreeSet<AssetId>)> = BTreeMap::new();
        let mut needed = BTreeSet::new();
        for (owner, events) in plan.beat_sounds() {
            for (sound, event) in events {
                let clocks = plan.beat_sound_clock_layouts(owner, sound)?;
                control.spend_plan_work(clocks.len().max(1))?;
                for (clock, layout) in clocks {
                    definitions
                        .entry(clock.clone())
                        .or_insert_with(|| (layout, BTreeSet::new()))
                        .1
                        .insert(event.source.asset.clone());
                }
                let scopes = plan.beat_sound_clock_scopes(
                    owner,
                    sound,
                    control.query_limits()?.maximum_work,
                )?;
                for scope in &scopes {
                    control.spend_plan_work(scope.proof_work())?;
                }
                if scopes.is_empty() {
                    continue;
                }
                let batch = current_occurrence_supports(
                    plan,
                    context,
                    owner,
                    event,
                    samples.clone(),
                    scopes.len(),
                    control,
                )?;
                for voice in batch.supports() {
                    for scope in &scopes {
                        let (instance, work) = scope.try_remap_instance_with_work(
                            voice.instance(),
                            control.query_limits()?.maximum_work,
                        )?;
                        control.spend_plan_work(work)?;
                        if instance.is_some() {
                            needed.insert(scope.timing().clone());
                            break;
                        }
                    }
                }
            }
        }
        let mut selected_assets = 0usize;
        for clock in &needed {
            selected_assets = selected_assets
                .checked_add(definitions[clock].1.len())
                .ok_or(StageAudioError::Limit("sound processing assets per read"))?;
            if selected_assets > deadpan_core::MAX_DOCUMENT_SOUNDS {
                return Err(StageAudioError::Limit("sound processing assets per read"));
            }
        }
        // Different birth cohorts can become first while seeking. Retain only
        // this bounded query's plans so a long journal cannot grow the cache.
        self.sound_processing_plans
            .retain(|clock, _| needed.contains(clock));
        let mut prepared = BTreeMap::new();
        for (clock, (layout, assets)) in definitions {
            if !needed.contains(&clock) {
                continue;
            }
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
    placements: &[deadpan_plan::AudioSoundPlacement],
    control: WorkControl<'_>,
) -> Result<AudioRoutedRoot<'plan>, StageAudioError> {
    let extent = original.extent();
    if placements.len() < 2
        || placements.first().is_none_or(|first| {
            first.allocation() != extent || first.complete_start() != extent.start
        })
    {
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
    let mut windows = Vec::with_capacity(placements.len());
    for (index, placement) in placements.iter().enumerate() {
        // Ripple admission examines its accumulated graph, so charge the
        // retained prefix as well as the newly appended operation.
        control.spend_plan_work(index + 1)?;
        grids.push(AudioSampleGrid::new(
            ExactRatio::ZERO.checked_sub(placement.complete_start())?,
            step,
            AudioBoundaryRule::RoundEven,
        )?);
        windows.push(deadpan_plan::AudioOccurrenceWindow {
            selection: placement
                .allocation()
                .start
                .checked_sub(placement.complete_start())?
                ..placement
                    .allocation()
                    .end
                    .checked_sub(placement.complete_start())?,
            anchor: placement.anchor(),
        });
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
    let route =
        AudioSoundRoute::<AudioSample>::new(route, grids)?.with_occurrence_windows(windows)?;
    Ok(AudioRoutedRoot::occurrence(original, route)?)
}

pub(super) fn placements<'query, 'current, 'original>(
    original: &'query AudioSourceOccurrence<'original>,
    current: &'query AudioSourceOccurrence<'current>,
    clocks: &'query [deadpan_plan::AudioSoundClockScope<'current>],
    context: Option<&'query deadpan_plan::AudioSoundOwnerContext<'current>>,
    control: WorkControl<'_>,
) -> Result<deadpan_plan::AudioSoundPlacementHistory<'query, 'current, 'original>, StageAudioError>
{
    let mut cursor = current.placement_history_cursor(original, clocks, context)?;
    while !cursor.is_complete() {
        let work = cursor.advance(control.query_limits()?.maximum_work)?;
        control.spend_plan_work(work)?;
    }
    Ok(cursor.finish()?)
}

#[cfg(test)]
#[path = "sound_clocks/tests.rs"]
mod tests;

pub(super) fn historical_occurrence<'plan>(
    plan: &'plan RenderPlan,
    event: &deadpan_core::BeatSound,
    instance: deadpan_core::InstancePath,
    control: WorkControl<'_>,
    context: Option<(
        &deadpan_plan::AudioSoundOwnerContext<'_>,
        &deadpan_plan::AudioSoundClockScope<'_>,
    )>,
) -> Result<AudioSourceOccurrence<'plan>, StageAudioError> {
    let historical = if let Some((context, scope)) = context {
        let (instance, work) =
            scope.try_remap_context_with_work(context, control.query_limits()?.maximum_work)?;
        control.spend_plan_work(work)?;
        let limits = control.query_limits()?;
        instance
            .map(|instance| plan.audio_sound_instance_context(&instance, limits))
            .transpose()?
    } else {
        None
    };
    let recipe = AudioSourceVoiceRecipe {
        source: event.source.clone(),
        mapping: event.mapping,
        offset: event.offset,
    };
    let voice = match historical {
        Some(context) => {
            control.spend_plan_work(context.construction_work())?;
            context.source_voice_occurrence(instance, recipe, control.query_limits()?)?
        }
        None => plan.source_voice_occurrence(instance, recipe, control.query_limits()?)?,
    };
    control.spend_plan_work(voice.construction_work())?;
    Ok(voice)
}

pub(super) fn current_occurrences<'plan>(
    plan: &'plan RenderPlan,
    context: Option<&deadpan_plan::AudioSoundOwnerContext<'plan>>,
    owner: &deadpan_core::NodeId,
    event: &deadpan_core::BeatSound,
    samples: Range<AudioSample>,
    clocks: usize,
    control: WorkControl<'_>,
) -> Result<deadpan_plan::AudioSourceOccurrences<'plan>, StageAudioError> {
    let recipe = AudioSourceVoiceRecipe {
        source: event.source.clone(),
        mapping: event.mapping,
        offset: event.offset,
    };
    let samples = occurrence_search_samples(plan, context, samples, clocks)?;
    let batch = match context {
        Some(context) => {
            context.source_voice_occurrences(owner, recipe, samples, control.query_limits()?)?
        }
        None => plan.source_voice_occurrences(owner, recipe, samples, control.query_limits()?)?,
    };
    control.spend_plan_work(batch.construction_work())?;
    Ok(batch)
}

/// Clock selection needs exact current paths and processed support, not a
/// retained PCM graph. Normal beat preparation still validates each full graph
/// before media access. Unsupported lightweight cases use Plan's full builder.
fn current_occurrence_supports(
    plan: &RenderPlan,
    context: Option<&deadpan_plan::AudioSoundOwnerContext<'_>>,
    owner: &deadpan_core::NodeId,
    event: &deadpan_core::BeatSound,
    samples: Range<AudioSample>,
    clocks: usize,
    control: WorkControl<'_>,
) -> Result<deadpan_plan::AudioSourceOccurrenceSupports, StageAudioError> {
    let recipe = AudioSourceVoiceRecipe {
        source: event.source.clone(),
        mapping: event.mapping,
        offset: event.offset,
    };
    let samples = occurrence_search_samples(plan, context, samples, clocks)?;
    let batch = match context {
        Some(context) => context.source_voice_occurrence_supports(
            owner,
            recipe,
            samples,
            control.query_limits()?,
        )?,
        None => {
            plan.source_voice_occurrence_supports(owner, recipe, samples, control.query_limits()?)?
        }
    };
    control.spend_plan_work(batch.construction_work())?;
    Ok(batch)
}

fn occurrence_search_samples(
    plan: &RenderPlan,
    context: Option<&deadpan_plan::AudioSoundOwnerContext<'_>>,
    samples: Range<AudioSample>,
    clocks: usize,
) -> Result<Range<AudioSample>, StageAudioError> {
    let Some(context) = context else {
        return Ok(samples);
    };
    if !context.belongs_to(plan) {
        return Err(StageAudioError::ForeignDomain);
    }
    // Each translated rounded boundary differs from its nominal affine
    // displacement by less than one sample. Search that bounded halo;
    // the retained route, not this conservative search, admits input.
    let margin = i64::try_from(clocks).map_err(|_| StageAudioError::Range)?;
    let support = context.samples();
    let start = AudioSample(samples.start.0.saturating_sub(margin))
        .max(support.start)
        .min(support.end);
    let end = AudioSample(samples.end.0.saturating_add(margin))
        .min(support.end)
        .max(start);
    Ok(start..end)
}
