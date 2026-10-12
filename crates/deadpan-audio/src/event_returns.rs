//! Independent finite returns are prepared on the original event input clock,
//! then transported by saved delivery routes. Current output gates stay outside.

use super::*;
use deadpan_core::{AudioEdgePolicy, AudioTreatmentStage, AudioTreatments, TailEffect};
use deadpan_plan::{AudioRoutedRootInput, AudioSoundRoute};

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct EventKey {
    // This cache lives only within one immutable preparation request. The
    // graph prefix and voice identify its checked source/occurrence recipe.
    parent: usize,
    voice: usize,
    effect: u8,
    tile: i64,
    allocation: (i64, i64),
    audible: (i64, i64),
    edges: (bool, bool),
}

pub(super) struct EventRead<'a> {
    pub parent: usize,
    pub voice: usize,
    pub start: AudioSample,
    pub frames: u32,
    pub treatments: &'a AudioTreatments,
    pub edges: (AudioEdgePolicy, AudioEdgePolicy),
    pub depth: usize,
}

pub(super) struct EventOutput {
    pub dry: Vec<[f64; 2]>,
    pub returned: WideBlock,
}

pub(super) fn needs_processing(recipe: &AudioTreatments) -> bool {
    !recipe.wet_sends().is_empty()
        || recipe.equalizer().is_some_and(|eq| !eq.is_unity())
        || recipe.saturation().is_some()
        || recipe
            .balance()
            .is_some_and(|balance| balance.percent() != 0)
        || recipe.clip_gain().is_some_and(|gain| {
            gain.muted() || !gain.envelopes().is_empty() || !gain.mute_ranges().is_empty()
        })
}

pub(super) struct EventSource<'a, 'plan> {
    pub input: &'a AudioRoutedRootInput<'plan>,
    pub route: Option<&'a AudioSoundRoute<AudioSample>>,
}

struct EventProgram<'a, 'plan> {
    input: &'a AudioRoutedRootInput<'plan>,
    key: &'a EventKey,
    recipe: &'a AudioTreatments,
}

impl StageAudio {
    pub(super) fn read_event_returns(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        graph: &mut Graph<'_>,
        source: EventSource<'_, '_>,
        request: EventRead<'_>,
        control: WorkControl<'_>,
    ) -> Result<EventOutput, StageAudioError> {
        let (allocation, audible) = match source.input {
            AudioRoutedRootInput::Source(input) => (input.samples()?, input.audible_samples()),
            AudioRoutedRootInput::Occurrence(input) => (input.samples(), input.audible_samples()?),
            AudioRoutedRootInput::Projected(_) => {
                return Err(PlanError::InvalidPlan(
                    "independent event return requires a source or owned occurrence",
                )
                .into());
            }
        };
        let end = AudioSample(
            request
                .start
                .0
                .checked_add(i64::from(request.frames))
                .ok_or(TimeError::Overflow)?,
        );
        let mut output = WideBlock {
            samples: vec![[0.0; 2]; request.frames as usize],
            suppressed: Vec::new(),
            dependencies: Dependencies::new(),
            relative_depth: 0,
        };
        let mut dry = vec![[0.0; 2]; request.frames as usize];
        self.preflight_retained_sound_input(source.input, control)?;
        let (dependencies, depth) =
            self.admit_retained_sound_input(provider, source.input, control)?;
        output.dependencies.extend(dependencies);
        output.relative_depth = depth;
        let spans = if let Some(route) = source.route {
            let query = route.query(request.start..end, control.query_limits()?)?;
            control.spend_plan_work(query.stats.work)?;
            let grid = route.recipe_grid();
            let retained = route.recipe_samples();
            query
                .spans
                .into_iter()
                .map(|span| {
                    let old = span
                        .sampling
                        .map(|sampling| {
                            super::super::routed::routed_old_range(
                                sampling.local_at(span.samples.start)?,
                                grid,
                                span.samples.end.0 - span.samples.start.0,
                                retained.start.0..retained.end.0,
                            )
                        })
                        .transpose()?;
                    Ok::<_, StageAudioError>((span.samples, old))
                })
                .collect::<Result<Vec<_>, _>>()?
        } else {
            vec![(request.start..end, Some(request.start.0..end.0))]
        };
        for (destination, old) in spans {
            let Some(old) = old else { continue };
            let template = EventKey {
                parent: request.parent,
                voice: request.voice,
                effect: 2,
                tile: 0,
                allocation: (allocation.start.0, allocation.end.0),
                audible: (audible.start.0, audible.end.0),
                edges: (
                    request.edges.0 == AudioEdgePolicy::Automatic,
                    request.edges.1 == AudioEdgePolicy::Automatic,
                ),
            };
            let block = self.event_prefix(
                provider,
                graph,
                EventProgram {
                    input: source.input,
                    key: &template,
                    recipe: request.treatments,
                },
                AudioSample(old.start)..AudioSample(old.end),
                request.treatments.order().len(),
                control,
            )?;
            let offset = (destination.start.0 - request.start.0) as usize;
            dry[offset..offset + block.samples.len()].copy_from_slice(&block.samples);
            output.dependencies.extend(block.dependencies);
            output.relative_depth = output.relative_depth.max(block.relative_depth);
            for (effect_id, effect) in [TailEffect::Reverb, TailEffect::Delay]
                .into_iter()
                .enumerate()
            {
                let Some(level) = request.treatments.wet_sends().level(effect) else {
                    continue;
                };
                let gain = 10f64.powf(f64::from(level.millidecibels()) / 20_000.0);
                let width = crate::wet_effect_descriptor(effect).output_frames as i64;
                let mut at = old.start;
                while at < old.end {
                    control.check()?;
                    let tile = at
                        .div_euclid(width)
                        .checked_mul(width)
                        .ok_or(TimeError::Overflow)?;
                    let key = EventKey {
                        parent: request.parent,
                        voice: request.voice,
                        effect: effect_id as u8,
                        tile,
                        allocation: (allocation.start.0, allocation.end.0),
                        audible: (audible.start.0, audible.end.0),
                        edges: (
                            request.edges.0 == AudioEdgePolicy::Automatic,
                            request.edges.1 == AudioEdgePolicy::Automatic,
                        ),
                    };
                    let block = self.event_return_tile(
                        provider,
                        graph,
                        source.input,
                        key,
                        request.treatments,
                        control,
                    )?;
                    if request
                        .depth
                        .checked_add(block.relative_depth)
                        .is_none_or(|depth| depth > self.limits.maximum_depth)
                    {
                        return Err(StageAudioError::Limit("nested stage depth"));
                    }
                    let until = old
                        .end
                        .min(tile.checked_add(width).ok_or(TimeError::Overflow)?);
                    for at in at..until {
                        let current = destination
                            .start
                            .0
                            .checked_add(at - old.start)
                            .ok_or(TimeError::Overflow)?;
                        let out = &mut output.samples[(current - request.start.0) as usize];
                        for (out, value) in out.iter_mut().zip(block.samples[(at - tile) as usize])
                        {
                            *out += value * gain;
                            if !out.is_finite() {
                                return Err(PreparationError::InvalidSamples.into());
                            }
                        }
                    }
                    output.dependencies.extend(block.dependencies.clone());
                    output.relative_depth = output.relative_depth.max(block.relative_depth);
                    at = until;
                }
            }
        }
        if request
            .depth
            .checked_add(output.relative_depth)
            .is_none_or(|depth| depth > self.limits.maximum_depth)
        {
            return Err(StageAudioError::Limit("nested stage depth"));
        }
        Ok(EventOutput {
            dry,
            returned: output,
        })
    }

    fn event_return_tile(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        graph: &mut Graph<'_>,
        input: &AudioRoutedRootInput<'_>,
        key: EventKey,
        recipe: &AudioTreatments,
        control: WorkControl<'_>,
    ) -> Result<Arc<WideBlock>, StageAudioError> {
        control.check()?;
        if let Some(block) = graph.event_tiles.get(&key) {
            return Ok(Arc::clone(block));
        }
        let mut node = key.clone();
        node.tile = 0;
        if !graph.event_nodes.contains(&node) {
            if graph.nodes.len() + graph.event_nodes.len()
                >= self.limits.maximum_prepared_stages as usize
            {
                return Err(StageAudioError::Limit("temporal owner count"));
            }
            graph.event_nodes.insert(node);
        }
        let (history, frames, width, until_stage) = if key.effect == 2 {
            let stage = recipe
                .order()
                .iter()
                .position(|stage| *stage == AudioTreatmentStage::Equalizer)
                .ok_or(PlanError::InvalidPlan("event EQ absent"))?;
            (RADIUS, INPUT, OUTPUT, stage)
        } else {
            let effect = if key.effect == 0 {
                TailEffect::Reverb
            } else {
                TailEffect::Delay
            };
            let shape = crate::wet_effect_descriptor(effect);
            (
                shape.history_frames,
                shape.history_frames + shape.output_frames,
                shape.output_frames,
                recipe.order().len(),
            )
        };
        if frames as u32 > self.limits.maximum_input_frames
            || width as u32 > self.limits.maximum_output_frames
        {
            return Err(StageAudioError::Limit("event return input/output frames"));
        }
        {
            let mut work = control.work.borrow_mut();
            work.prepared_frames = work
                .prepared_frames
                .checked_add((frames + width) as u64)
                .ok_or(StageAudioError::Limit("prepared frames per read"))?;
            if work.prepared_frames > self.limits.maximum_prepared_frames {
                return Err(StageAudioError::Limit("prepared frames per read"));
            }
        }
        let begin = key
            .tile
            .checked_sub(history as i64)
            .ok_or(TimeError::Overflow)?;
        let end = begin
            .checked_add(frames as i64)
            .ok_or(TimeError::Overflow)?;
        let reservation = ((frames + width * 2) * 2) as u64 + u64::from(MAX_OUTPUT_FRAMES) * 8;
        self.make_room(reservation, false, control)?;
        self.active_frames += reservation;
        let result = (|| {
            let mut left = vec![0.0; frames];
            let mut right = vec![0.0; frames];
            let mut dependencies = Dependencies::new();
            let mut relative_depth = 1;
            let mut at = begin.max(key.allocation.0).max(key.audible.0);
            let until = end.min(key.allocation.1).min(key.audible.1);
            while at < until {
                let count = u32::try_from((until - at).min(i64::from(MAX_OUTPUT_FRAMES)))
                    .map_err(|_| StageAudioError::Range)?;
                let raw = self.event_prefix(
                    provider,
                    graph,
                    EventProgram {
                        input,
                        key: &key,
                        recipe,
                    },
                    AudioSample(at)..AudioSample(at + i64::from(count)),
                    until_stage,
                    control,
                )?;
                for (index, sample) in raw.samples.iter().enumerate() {
                    let at = at.checked_add(index as i64).ok_or(TimeError::Overflow)?;
                    left[(at - begin) as usize] = sample[0];
                    right[(at - begin) as usize] = sample[1];
                }
                dependencies.extend(raw.dependencies);
                relative_depth = relative_depth.max(raw.relative_depth + 1);
                at += i64::from(count);
            }
            let mut out_left = vec![0.0; width];
            let mut out_right = vec![0.0; width];
            if key.effect == 2 {
                let mut node = key.clone();
                node.tile = 0;
                if !graph.event_filters.contains_key(&node) {
                    control.spend_plan_work(deadpan_dsp::FINITE_FILTER_TAPS)?;
                    let resident = (crate::Equalizer::preparation_bytes_bound() as u64).div_ceil(8);
                    self.make_room(resident, false, control)?;
                    let filter = crate::Equalizer::new(
                        recipe
                            .equalizer()
                            .ok_or(PlanError::InvalidPlan("event EQ absent"))?,
                        control.cancelled,
                    )?;
                    self.active_frames += resident;
                    graph.resident += resident;
                    graph.event_filters.insert(node.clone(), filter);
                }
                graph
                    .event_filters
                    .get_mut(&node)
                    .ok_or(PlanError::InvalidPlan("event EQ absent"))?
                    .process_tile(
                        &left,
                        &right,
                        &mut out_left,
                        &mut out_right,
                        control.cancelled,
                    )?;
            } else {
                self.process_shared_wet(
                    graph,
                    if key.effect == 0 {
                        TailEffect::Reverb
                    } else {
                        TailEffect::Delay
                    },
                    (&left, &right),
                    (&mut out_left, &mut out_right),
                    control,
                )?;
            }
            control.check()?;
            let resident = (width * 2) as u64;
            self.make_room(resident, false, control)?;
            self.active_frames += resident;
            graph.resident += resident;
            let block = Arc::new(WideBlock {
                samples: out_left
                    .into_iter()
                    .zip(out_right)
                    .map(|(l, r)| [l, r])
                    .collect(),
                suppressed: Vec::new(),
                dependencies,
                relative_depth,
            });
            graph.event_tiles.insert(key, Arc::clone(&block));
            Ok(block)
        })();
        self.active_frames -= reservation;
        result
    }

    /// Evaluate a strict prefix in the complete retained event clock. Temporal
    /// tiles recurse only to earlier stages; delivery cuts never become history.
    fn event_prefix(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        graph: &mut Graph<'_>,
        program: EventProgram<'_, '_>,
        samples: Range<AudioSample>,
        until: usize,
        control: WorkControl<'_>,
    ) -> Result<WideBlock, StageAudioError> {
        control.check()?;
        let EventProgram {
            input,
            key: template,
            recipe,
        } = program;
        let start = samples.start;
        let end = samples.end.0;
        let frames = u32::try_from(end.checked_sub(start.0).ok_or(TimeError::Overflow)?)
            .map_err(|_| StageAudioError::Range)?;
        if frames > MAX_OUTPUT_FRAMES {
            return Err(StageAudioError::Range);
        }
        let clock = match input {
            AudioRoutedRootInput::Source(source) => source.treatment_clock(),
            AudioRoutedRootInput::Occurrence(source) => source.treatment_clock()?,
            AudioRoutedRootInput::Projected(_) => {
                return Err(PlanError::InvalidPlan("event clock absent").into());
            }
        };
        let eq = recipe.order()[..until]
            .iter()
            .position(|stage| *stage == AudioTreatmentStage::Equalizer)
            .filter(|_| recipe.equalizer().is_some_and(|eq| !eq.is_unity()));
        let mut output = WideBlock {
            samples: vec![[0.0; 2]; frames as usize],
            suppressed: Vec::new(),
            dependencies: Dependencies::new(),
            relative_depth: 0,
        };
        let mut at = start.0.max(template.allocation.0).max(template.audible.0);
        let stop = end.min(template.allocation.1).min(template.audible.1);
        let length = u64::try_from(i128::from(template.audible.1) - i128::from(template.audible.0))
            .map_err(|_| StageAudioError::Range)?;
        while at < stop {
            let count = (stop - at).min(i64::from(MAX_OUTPUT_FRAMES)) as u32;
            if eq.is_some() {
                let tile = at
                    .div_euclid(OUTPUT as i64)
                    .checked_mul(OUTPUT as i64)
                    .ok_or(TimeError::Overflow)?;
                let count = count.min((tile + OUTPUT as i64 - at) as u32);
                let mut key = template.clone();
                key.effect = 2;
                key.tile = tile;
                let block = self.event_return_tile(provider, graph, input, key, recipe, control)?;
                for offset in 0..count as usize {
                    output.samples[(at - start.0) as usize + offset] =
                        block.samples[(at - tile) as usize + offset];
                }
                output.dependencies.extend(block.dependencies.clone());
                output.relative_depth = output.relative_depth.max(block.relative_depth);
                at += i64::from(count);
            } else {
                let query =
                    self.prepare_retained_sound_query(input, AudioSample(at), count, control)?;
                let block = self.read_retained_sound_query(provider, input, query, control)?;
                for (offset, sample) in block.samples.iter().enumerate() {
                    let label = at + offset as i64;
                    if permitted(&block, AudioSample(label)) {
                        let edge = crate::edges::edge_gain(
                            length,
                            i128::from(label) - i128::from(template.audible.0),
                            template.edges.0,
                            template.edges.1,
                        );
                        output.samples[(label - start.0) as usize] =
                            sample.map(|v| f64::from(v * edge));
                    }
                }
                output.dependencies.extend(block.dependencies);
                output.relative_depth = output.relative_depth.max(block.relative_depth);
                at += i64::from(count);
            }
        }
        let from = eq.map_or(0, |index| index + 1);
        for (offset, value) in output.samples.iter_mut().enumerate() {
            let at = start.0 + offset as i64;
            if at < template.audible.0
                || at >= template.audible.1
                || at < template.allocation.0
                || at >= template.allocation.1
            {
                continue;
            }
            let mut gain = authored_gain::GainSum::default();
            gain.treatment(
                recipe,
                clock.local_at(AudioSample(at))?,
                control,
                from..until,
            )?;
            *value = gain.apply_wide(*value)?;
        }
        Ok(output)
    }

    pub(super) fn process_shared_wet(
        &mut self,
        graph: &mut Graph<'_>,
        effect: TailEffect,
        input: (&[f64], &[f64]),
        output: (&mut [f64], &mut [f64]),
        control: WorkControl<'_>,
    ) -> Result<(), StageAudioError> {
        let slot = match effect {
            TailEffect::Reverb => 0,
            TailEffect::Delay => 1,
        };
        if graph.wet_filters[slot].is_none() {
            let resident =
                (crate::FiniteWetEffect::preparation_bytes_bound(effect) as u64).div_ceil(8);
            control.spend_plan_work(crate::wet_effect_descriptor(effect).history_frames + 1)?;
            self.make_room(resident, false, control)?;
            let processor = crate::FiniteWetEffect::new(effect, control.cancelled)?;
            control.check()?;
            self.active_frames += resident;
            graph.resident += resident;
            graph.wet_filters[slot] = Some(processor);
        }
        graph.wet_filters[slot]
            .as_mut()
            .ok_or(PlanError::InvalidPlan("wet processor absent"))?
            .process_tile(input.0, input.1, output.0, output.1, control.cancelled)?;
        Ok(())
    }
}
