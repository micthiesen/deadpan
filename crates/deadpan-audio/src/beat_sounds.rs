//! Saved independent voices, instantiated from their current structural owners.

use super::*;
use deadpan_core::{AudioEdgePolicy, BeatSound};
use deadpan_plan::AudioSourceOccurrence;

pub(super) struct PreparedBeatSound<'plan, 'history> {
    pub(super) event: &'plan BeatSound,
    pub(super) owner: &'plan deadpan_core::NodeId,
    pub(super) id: &'plan deadpan_core::SoundId,
    pub(super) occurrences: Vec<PreparedOccurrence<'plan, 'history>>,
}

pub(super) struct PreparedOccurrence<'plan, 'history> {
    pub(super) voice: AudioSourceOccurrence<'plan>,
    pub(super) samples: Range<AudioSample>,
    pub(super) input: PreparedBeatInput<'plan, 'history>,
    pub(super) fades: Vec<deadpan_plan::AudioSoundGateSpan>,
    pub(super) post_fades: Vec<deadpan_plan::AudioSoundGateSpan>,
    mix_route: Option<super::group_mix::Route<'plan>>,
}

pub(super) enum PreparedBeatInput<'plan, 'history> {
    Current(Box<RootReadQueries<'plan>>),
    Routed {
        plan: Arc<RenderPlan>,
        query: super::routed::PreparedRoutedRootQuery<'history>,
    },
}

impl<'plan> PreparedBeatSound<'plan, '_> {
    pub(super) fn route_mix(
        &mut self,
        mix: &mut super::group_mix::GroupMix<'plan>,
        control: WorkControl<'_>,
    ) -> Result<(), StageAudioError> {
        for occurrence in &mut self.occurrences {
            occurrence.mix_route = Some(mix.route(
                occurrence.voice.gain_owners(),
                occurrence.samples.clone(),
                control,
            )?);
        }
        Ok(())
    }

    pub(super) fn has_routed(&self) -> bool {
        self.occurrences
            .iter()
            .any(|occurrence| matches!(&occurrence.input, PreparedBeatInput::Routed { .. }))
    }
}

impl StageAudio {
    pub(super) fn prepare_beat_sounds<'plan, 'history>(
        &mut self,
        plan: &'plan RenderPlan,
        retained: &'history super::sound_clocks::SoundProcessingPlans,
        samples: Range<AudioSample>,
        control: WorkControl<'_>,
        context: Option<&deadpan_plan::AudioSoundOwnerContext<'plan>>,
    ) -> Result<Vec<PreparedBeatSound<'plan, 'history>>, StageAudioError> {
        let mut prepared = Vec::new();
        for (owner, events) in plan.beat_sounds() {
            for (sound, event) in events {
                control.check()?;
                control.admit_dependency(&event.source.asset)?;
                let scopes = plan.beat_sound_clock_scopes(
                    owner,
                    sound,
                    control.query_limits()?.maximum_work,
                )?;
                for scope in &scopes {
                    control.spend_plan_work(scope.proof_work())?;
                }
                let mut bindings = BTreeMap::new();
                let batch = super::sound_clocks::current_occurrences(
                    plan,
                    context,
                    owner,
                    event,
                    samples.clone(),
                    scopes.len(),
                    control,
                )?;
                let mut occurrences = Vec::with_capacity(batch.voices().len());
                for voice in batch.voices() {
                    control.check()?;
                    let extent = voice.samples();
                    let interval = samples.start.max(extent.start)..samples.end.min(extent.end);
                    if interval.start >= interval.end && context.is_none() {
                        continue;
                    }
                    deadpan_core::validate_audio_treatment_layers(
                        voice
                            .gain_owners()
                            .iter()
                            .filter_map(|owner| owner.treatments()),
                    )?;
                    let mut first = None;
                    for (index, scope) in scopes.iter().enumerate() {
                        let (instance, work) = scope.try_remap_instance_with_work(
                            voice.instance(),
                            control.query_limits()?.maximum_work,
                        )?;
                        control.spend_plan_work(work)?;
                        if let Some(instance) = instance {
                            first = Some((index, scope, instance));
                            break;
                        }
                    }
                    let (input, fades, post_fades, interval) =
                        if let Some((index, first_scope, historical_instance)) = first {
                            let first_plan = retained.get(first_scope.timing()).ok_or(
                                PlanError::InvalidPlan("sound processing layout was not prepared"),
                            )?;
                            if let std::collections::btree_map::Entry::Vacant(entry) =
                                bindings.entry(index)
                            {
                                let (binding, work) = first_scope.bind_historical_plan(
                                    first_plan,
                                    control.query_limits()?.maximum_work,
                                )?;
                                control.spend_plan_work(work)?;
                                entry.insert(binding);
                            }
                            let binding = &bindings[&index];
                            let original = super::sound_clocks::historical_occurrence(
                                first_plan,
                                event,
                                historical_instance,
                                control,
                                context.map(|context| (context, first_scope)),
                            )?;
                            let (alias, alias_work) = binding.alias_occurrence(
                                &original,
                                voice,
                                control.query_limits()?.maximum_work,
                            )?;
                            control.spend_plan_work(alias_work)?;
                            let placements = super::sound_clocks::placements(
                                &original, voice, &scopes, context, control,
                            )?;
                            let mut routed = super::sound_clocks::route_occurrence(
                                original.clone(),
                                &placements,
                                control,
                            )?;
                            let history = if placements.iter().all(|p| p.expands_history) {
                                control.spend_plan_work(placements.len())?;
                                voice.complete_processing_history(
                                    &original,
                                    &alias,
                                    routed.route(),
                                )?
                            } else {
                                None
                            };
                            if let Some(history) = &history {
                                routed = deadpan_plan::AudioRoutedRoot::occurrence(
                                    original.clone(),
                                    history.route().clone(),
                                )?;
                            }
                            let support = routed.route().samples();
                            let interval =
                                samples.start.max(support.start)..samples.end.min(support.end);
                            if interval.start >= interval.end {
                                continue;
                            }
                            let gate_fades = |edges| {
                                if let Some(history) = &history {
                                    history
                                        .gate_fades(
                                            edges,
                                            interval.clone(),
                                            control.query_limits()?,
                                        )
                                        .map_err(StageAudioError::from)
                                } else {
                                    voice
                                        .routed_window_gate_fades_from(
                                            &original,
                                            &alias,
                                            routed.route(),
                                            edges,
                                            interval.clone(),
                                            control.query_limits()?,
                                        )
                                        .map_err(StageAudioError::from)
                                }
                            };
                            let fades = gate_fades((event.start_edge, event.end_edge))?;
                            control.spend_plan_work(fades.work)?;
                            let post_fades = gate_fades((
                                deadpan_core::AudioEdgePolicy::Hard,
                                deadpan_core::AudioEdgePolicy::Hard,
                            ))?;
                            control.spend_plan_work(post_fades.work)?;
                            let query = self.with_plan_scope(Arc::clone(first_plan), |reader| {
                                reader.preflight_routed_root(
                                    &routed,
                                    interval.start,
                                    count(&interval)?,
                                    control,
                                )
                            })?;
                            (
                                PreparedBeatInput::Routed {
                                    plan: Arc::clone(first_plan),
                                    query,
                                },
                                fades,
                                post_fades,
                                interval,
                            )
                        } else {
                            if interval.start >= interval.end {
                                continue;
                            }
                            let queries =
                                self.source_occurrence_queries(voice, interval.clone(), control)?;
                            self.preflight_root_queries(
                                &queries,
                                interval.start,
                                count(&interval)?,
                                control,
                                0,
                            )?;
                            let fades = voice.gate_fades(
                                event.start_edge,
                                event.end_edge,
                                interval.clone(),
                                control.query_limits()?,
                            )?;
                            control.spend_plan_work(fades.work)?;
                            let post_fades = voice.gate_fades(
                                deadpan_core::AudioEdgePolicy::Hard,
                                deadpan_core::AudioEdgePolicy::Hard,
                                interval.clone(),
                                control.query_limits()?,
                            )?;
                            control.spend_plan_work(post_fades.work)?;
                            (
                                PreparedBeatInput::Current(Box::new(queries)),
                                fades,
                                post_fades,
                                interval,
                            )
                        };
                    sound_events::validate_gate_envelopes(&fades.spans, interval.clone())?;
                    occurrences.push(PreparedOccurrence {
                        voice: voice.clone(),
                        samples: interval,
                        input,
                        fades: fades.spans,
                        post_fades: post_fades.spans,
                        mix_route: None,
                    });
                }
                prepared.push(PreparedBeatSound {
                    event,
                    owner,
                    id: sound,
                    occurrences,
                });
            }
        }
        Ok(prepared)
    }

    pub(super) fn read_prepared_beat_sound(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        input: PreparedBeatInput<'_, '_>,
        samples: Range<AudioSample>,
        control: WorkControl<'_>,
    ) -> Result<ReadBlock, StageAudioError> {
        match input {
            PreparedBeatInput::Current(queries) => self.read_queries(
                provider,
                samples.start,
                count(&samples)?,
                control,
                0,
                *queries,
            ),
            PreparedBeatInput::Routed { plan, query } => self.with_plan_scope(plan, |reader| {
                reader.read_routed_root_controlled(provider, query, control)
            }),
        }
    }

    pub(super) fn render_beat_sounds(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        original: &mut ReadBlock,
        mix: &mut super::group_mix::GroupMix<'_>,
        sounds: Vec<PreparedBeatSound<'_, '_>>,
        control: WorkControl<'_>,
    ) -> Result<(), StageAudioError> {
        let end = AudioSample(
            original
                .start
                .0
                .checked_add(
                    i64::try_from(original.samples.len()).map_err(|_| StageAudioError::Range)?,
                )
                .ok_or(StageAudioError::Range)?,
        );
        for sound in sounds {
            control.check()?;
            for occurrence in sound.occurrences {
                control.spend_plan_work(count(&occurrence.samples)? as usize)?;
                let mut block = self.read_prepared_beat_sound(
                    provider,
                    occurrence.input,
                    occurrence.samples.clone(),
                    control,
                )?;
                let offset = usize::try_from(occurrence.samples.start.0 - original.start.0)
                    .map_err(|_| StageAudioError::Range)?;
                let expected = count(&occurrence.samples)? as usize;
                if block.samples.len() != expected {
                    return Err(PlanError::InvalidPlan("incomplete beat sound PCM").into());
                }
                block.suppressed.extend(
                    occurrence
                        .fades
                        .iter()
                        .filter(|span| span.length == 0)
                        .map(|span| span.samples.clone()),
                );
                if original.start < occurrence.samples.start {
                    block
                        .suppressed
                        .push(original.start..occurrence.samples.start);
                }
                if occurrence.samples.end < end {
                    block.suppressed.push(occurrence.samples.end..end);
                }
                block.suppressed = merged_suppression(block.suppressed);
                let edges = occurrence.fades.into_iter().flat_map(|span| {
                    (0..span.samples.end.0 - span.samples.start.0).map(move |index| {
                        crate::edges::edge_gain(
                            span.length,
                            i128::from(span.progress_at_start) + i128::from(index),
                            span.start_edge == AudioEdgePolicy::Automatic,
                            span.end_edge == AudioEdgePolicy::Automatic,
                        )
                    })
                });
                let route = occurrence.mix_route.ok_or(PlanError::InvalidPlan(
                    "beat sound mix route was not admitted",
                ))?;
                for (index, (sample, edge)) in block.samples.into_iter().zip(edges).enumerate() {
                    mix.add(
                        &route,
                        offset + index,
                        sample.map(|value| value * edge),
                        sound.event.treatments.trim().millidecibels(),
                        control,
                    )?;
                }
                original.suppressed =
                    sound_events::intersect_suppression(&original.suppressed, &block.suppressed);
                original.relative_depth = original.relative_depth.max(block.relative_depth);
                original.dependencies.extend(block.dependencies);
            }
            // Missing occurrences and complete silence still depend on the
            // recipe. A warm limiter block cannot hide a revoked source.
            let asset = &sound.event.source.asset;
            let source = resolve_source(provider, &self.plan, asset, control.cancelled)?;
            original
                .dependencies
                .insert(asset.clone(), control.observe(asset, source)?);
        }
        Ok(())
    }
}
