//! Saved independent voices, instantiated from their current structural owners.

use super::*;
use deadpan_core::{AudioEdgePolicy, BeatSound};
use deadpan_plan::{AudioSourceOccurrence, AudioSourceVoiceRecipe};

pub(super) struct PreparedBeatSound<'plan> {
    event: &'plan BeatSound,
    occurrences: Vec<PreparedOccurrence<'plan>>,
}

struct PreparedOccurrence<'plan> {
    voice: AudioSourceOccurrence<'plan>,
    samples: Range<AudioSample>,
    input: PreparedBeatInput<'plan>,
    fades: Vec<deadpan_plan::AudioSoundGateSpan>,
}

enum PreparedBeatInput<'plan> {
    Current(Box<RootReadQueries<'plan>>),
    Routed {
        plan: Arc<RenderPlan>,
        query: super::routed::PreparedRoutedRootQuery<'plan>,
    },
}

impl PreparedBeatSound<'_> {
    pub(super) fn has_routed(&self) -> bool {
        self.occurrences
            .iter()
            .any(|occurrence| matches!(&occurrence.input, PreparedBeatInput::Routed { .. }))
    }
}

impl StageAudio {
    pub(super) fn prepare_beat_sounds<'plan>(
        &mut self,
        plan: &'plan RenderPlan,
        retained: &'plan super::sound_clocks::SoundProcessingPlans,
        samples: Range<AudioSample>,
        control: WorkControl<'_>,
    ) -> Result<Vec<PreparedBeatSound<'plan>>, StageAudioError> {
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
                let batch = plan.source_voice_occurrences(
                    owner,
                    AudioSourceVoiceRecipe {
                        source: event.source.clone(),
                        mapping: event.mapping,
                        offset: event.offset,
                    },
                    samples.clone(),
                    control.query_limits()?,
                )?;
                control.spend_plan_work(batch.construction_work())?;
                let mut occurrences = Vec::with_capacity(batch.voices().len());
                for voice in batch.voices() {
                    control.check()?;
                    let extent = voice.samples();
                    let interval = samples.start.max(extent.start)..samples.end.min(extent.end);
                    if interval.start >= interval.end {
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
                    let (input, fades) = if let Some((index, first_scope, historical_instance)) =
                        first
                    {
                        let first_plan =
                            retained
                                .get(first_scope.timing())
                                .ok_or(PlanError::InvalidPlan(
                                    "sound processing layout was not prepared",
                                ))?;
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
                        )?;
                        let (alias, alias_work) = binding.alias_occurrence(
                            &original,
                            voice,
                            control.query_limits()?.maximum_work,
                        )?;
                        control.spend_plan_work(alias_work)?;
                        let placements =
                            super::sound_clocks::placements(&original, voice, &scopes, control)?;
                        let fades = voice.routed_gate_fades_from(
                            &original,
                            &alias,
                            &placements,
                            (event.start_edge, event.end_edge),
                            interval.clone(),
                            control.query_limits()?,
                        )?;
                        control.spend_plan_work(fades.work)?;
                        let routed =
                            super::sound_clocks::route_occurrence(original, &placements, control)?;
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
                        )
                    } else {
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
                        (PreparedBeatInput::Current(Box::new(queries)), fades)
                    };
                    sound_events::validate_gate_envelopes(&fades.spans, interval.clone())?;
                    occurrences.push(PreparedOccurrence {
                        voice: voice.clone(),
                        samples: interval,
                        input,
                        fades: fades.spans,
                    });
                }
                prepared.push(PreparedBeatSound { event, occurrences });
            }
        }
        Ok(prepared)
    }

    pub(super) fn render_beat_sounds(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        original: &mut ReadBlock,
        sum: &mut [[f64; 2]],
        sounds: Vec<PreparedBeatSound<'_>>,
        control: WorkControl<'_>,
        authored_gain: bool,
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
                let mut block = match occurrence.input {
                    PreparedBeatInput::Current(queries) => self.read_queries(
                        provider,
                        occurrence.samples.start,
                        count(&occurrence.samples)?,
                        control,
                        0,
                        *queries,
                    )?,
                    PreparedBeatInput::Routed { plan, query } => self
                        .with_plan_scope(plan, |reader| {
                            reader.read_routed_root_controlled(provider, query, control)
                        })?,
                };
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
                for (index, ((total, sample), edge)) in sum[offset..offset + expected]
                    .iter_mut()
                    .zip(block.samples)
                    .zip(edges)
                    .enumerate()
                {
                    let at = AudioSample(
                        occurrence
                            .samples
                            .start
                            .0
                            .checked_add(i64::try_from(index).map_err(|_| StageAudioError::Range)?)
                            .ok_or(StageAudioError::Range)?,
                    );
                    let gain = authored_gain::beat_sound_gain(
                        occurrence.voice.gain_owners(),
                        at,
                        sound.event.gain_millidecibels,
                        control,
                        authored_gain,
                    )?;
                    let gained = gain.apply(sample.map(|value| value * edge))?;
                    for channel in 0..2 {
                        total[channel] += gained[channel];
                    }
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
