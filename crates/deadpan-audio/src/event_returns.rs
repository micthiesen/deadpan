//! Independent finite returns are prepared on the original event input clock,
//! then transported by saved delivery routes. Current output gates stay outside.

use super::*;
use deadpan_core::{AudioEdgePolicy, TailEffect, TailSends};
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

pub(super) struct EventRead {
    pub parent: usize,
    pub voice: usize,
    pub start: AudioSample,
    pub frames: u32,
    pub sends: TailSends,
    pub edges: (AudioEdgePolicy, AudioEdgePolicy),
    pub depth: usize,
}

pub(super) struct EventSource<'a, 'plan> {
    pub input: &'a AudioRoutedRootInput<'plan>,
    pub route: Option<&'a AudioSoundRoute<AudioSample>>,
}

impl StageAudio {
    pub(super) fn read_event_returns(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        graph: &mut Graph<'_>,
        source: EventSource<'_, '_>,
        request: EventRead,
        control: WorkControl<'_>,
    ) -> Result<WideBlock, StageAudioError> {
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
        if request.sends.is_empty() {
            return Ok(output);
        }
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
            for (effect_id, effect) in [TailEffect::Reverb, TailEffect::Delay]
                .into_iter()
                .enumerate()
            {
                let Some(level) = request.sends.level(effect) else {
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
                    let block =
                        self.event_return_tile(provider, graph, source.input, key, control)?;
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
        Ok(output)
    }

    fn event_return_tile(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        graph: &mut Graph<'_>,
        input: &AudioRoutedRootInput<'_>,
        key: EventKey,
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
        let effect = if key.effect == 0 {
            TailEffect::Reverb
        } else {
            TailEffect::Delay
        };
        let shape = crate::wet_effect_descriptor(effect);
        let frames = shape.history_frames + shape.output_frames;
        let width = shape.output_frames;
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
            .checked_sub(shape.history_frames as i64)
            .ok_or(TimeError::Overflow)?;
        let end = key
            .tile
            .checked_add(width as i64)
            .ok_or(TimeError::Overflow)?;
        let reservation = ((frames + width * 2) * 2) as u64;
        self.make_room(reservation, false, control)?;
        self.active_frames += reservation;
        let result = (|| {
            let mut left = vec![0.0; frames];
            let mut right = vec![0.0; frames];
            let mut dependencies = Dependencies::new();
            let mut relative_depth = 1;
            let mut at = begin.max(key.allocation.0).max(key.audible.0);
            let until = end.min(key.allocation.1).min(key.audible.1);
            let length = u64::try_from(i128::from(key.audible.1) - i128::from(key.audible.0))
                .map_err(|_| StageAudioError::Range)?;
            while at < until {
                let count = u32::try_from((until - at).min(i64::from(MAX_OUTPUT_FRAMES)))
                    .map_err(|_| StageAudioError::Range)?;
                let query =
                    self.prepare_retained_sound_query(input, AudioSample(at), count, control)?;
                let raw = self.read_retained_sound_query(provider, input, query, control)?;
                for (index, sample) in raw.samples.iter().enumerate() {
                    let at = at.checked_add(index as i64).ok_or(TimeError::Overflow)?;
                    if permitted(&raw, AudioSample(at)) {
                        let gain = crate::edges::edge_gain(
                            length,
                            i128::from(at) - i128::from(key.audible.0),
                            key.edges.0,
                            key.edges.1,
                        );
                        left[(at - begin) as usize] = f64::from(sample[0]) * f64::from(gain);
                        right[(at - begin) as usize] = f64::from(sample[1]) * f64::from(gain);
                    }
                }
                dependencies.extend(raw.dependencies);
                relative_depth = relative_depth.max(raw.relative_depth + 1);
                at += i64::from(count);
            }
            let mut out_left = vec![0.0; width];
            let mut out_right = vec![0.0; width];
            self.process_shared_wet(
                graph,
                effect,
                (&left, &right),
                (&mut out_left, &mut out_right),
                control,
            )?;
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
