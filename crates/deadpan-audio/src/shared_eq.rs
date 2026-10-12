//! Recursive shared temporal processing. An EQ tile evaluates the complete
//! ordered bus prefix with the consuming sample's allowed contribution set.
//! This keeps silence permissions distinct. Selected contribution reads retain
//! the common saturation drive from all permitted inputs.

use super::authored_eq::Context;
use super::shared_mix::{Boundary, OwnerKey, Permissions, SharedMix};
use super::*;
use deadpan_core::{NodeId, SoundId};
use deadpan_dsp::{
    FINITE_FILTER_INPUT_FRAMES as INPUT, FINITE_FILTER_OUTPUT_FRAMES as OUTPUT,
    FINITE_FILTER_RADIUS as RADIUS,
};
use deadpan_plan::{
    AudioOwnerClock, AudioOwnerSupport, AudioSoundOwnerContext, AudioTreatmentBoundary,
};

#[derive(Clone)]
enum Sounds<'plan> {
    Root,
    Owner(Arc<AudioSoundOwnerContext<'plan>>),
    None,
}
#[derive(Clone)]
struct Contexts<'plan> {
    original: Context<'plan>,
    sounds: Sounds<'plan>,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Voice {
    Original,
    Root(SoundId),
    Beat(NodeId, SoundId, InstancePath),
}

struct WideBlock {
    samples: Vec<[f64; 2]>,
    suppressed: Vec<Range<AudioSample>>,
    dependencies: Dependencies,
    relative_depth: usize,
}
struct Node<'plan> {
    parent: usize,
    context: Contexts<'plan>,
    boundary: AudioTreatmentBoundary<'plan>,
    stop: Boundary,
    filter: Option<crate::Equalizer>,
    tiles: BTreeMap<(i64, Permissions, Option<usize>), Arc<WideBlock>>,
}
struct Graph<'plan> {
    plan: &'plan RenderPlan,
    nodes: Vec<Node<'plan>>,
    resident: u64,
    voices: BTreeMap<Voice, usize>,
    sends: Option<deadpan_core::TailEffect>,
    skip: Vec<authored_gain::OwnerIdentity>,
}

#[derive(Default)]
pub(super) struct InputSelection<'a> {
    pub(super) depth: usize,
    pub(super) sends: Option<deadpan_core::TailEffect>,
    pub(super) skip: &'a [authored_gain::OwnerIdentity],
}
impl Graph<'_> {
    fn voice(&mut self, voice: Voice) -> Result<usize, StageAudioError> {
        if let Some(id) = self.voices.get(&voice) {
            return Ok(*id);
        }
        if self.voices.len() == shared_mix::MAX_VOICES {
            return Err(StageAudioError::Limit("shared EQ voice count"));
        }
        let id = self.voices.len();
        self.voices.insert(voice, id);
        Ok(id)
    }
}
#[derive(Clone, Copy)]
struct Program<'a, 'plan> {
    context: &'a Contexts<'plan>,
    stop: Option<&'a Boundary>,
    allowed: Option<Permissions>,
    output_voice: Option<usize>,
    parent: usize,
    depth: usize,
}
fn permitted(block: &ReadBlock, at: AudioSample) -> bool {
    !block
        .suppressed
        .iter()
        .chain(&block.exhausted)
        .any(|range| range.contains(&at))
}
fn sound_context<'a, 'plan>(
    sounds: &'a Sounds<'plan>,
) -> Option<&'a AudioSoundOwnerContext<'plan>> {
    match sounds {
        Sounds::Owner(context) => Some(context),
        _ => None,
    }
}

impl StageAudio {
    pub(super) fn read_shared_equalizers(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        start: AudioSample,
        frames: u32,
        control: WorkControl<'_>,
    ) -> Result<ReadBlock, StageAudioError> {
        self.read_shared_input(provider, start, frames, control, InputSelection::default())
    }

    pub(super) fn read_shared_input(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        start: AudioSample,
        frames: u32,
        control: WorkControl<'_>,
        selection: InputSelection<'_>,
    ) -> Result<ReadBlock, StageAudioError> {
        control.check()?;
        if frames == 0 || frames > MAX_EDGE_PREPARATION_FRAMES {
            return Err(StageAudioError::PreparationRange);
        }
        let end = start
            .0
            .checked_add(i64::from(frames))
            .ok_or(TimeError::Overflow)?;
        let plan = Arc::clone(&self.plan);
        let mut graph = Graph {
            plan: &plan,
            nodes: Vec::new(),
            resident: 0,
            voices: BTreeMap::new(),
            sends: selection.sends,
            skip: selection.skip.to_vec(),
        };
        let reservation = u64::from(frames) * 3;
        self.make_room(reservation, false, control)?;
        self.active_frames += reservation;
        let result = (|| {
            let context = Contexts {
                original: Context::Root,
                sounds: Sounds::Root,
            };
            let program = Program {
                context: &context,
                stop: None,
                allowed: None,
                output_voice: None,
                parent: usize::MAX,
                depth: selection.depth,
            };
            let mut result = ReadBlock {
                start,
                samples: vec![[0.0; 2]; frames as usize],
                suppressed: Vec::new(),
                exhausted: Vec::new(),
                dependencies: Dependencies::new(),
                relative_depth: 0,
            };
            let mut at = start.0;
            while at < end {
                let count = u32::try_from((end - at).min(i64::from(MAX_OUTPUT_FRAMES)))
                    .map_err(|_| StageAudioError::Range)?;
                let block = self.read_shared_program(
                    provider,
                    &mut graph,
                    program,
                    AudioSample(at),
                    count,
                    control,
                )?;
                let offset = (at - start.0) as usize;
                authored_gain::write_finite_samples(
                    &mut result.samples[offset..offset + count as usize],
                    block.samples,
                )?;
                result.suppressed.extend(block.suppressed);
                result.dependencies.extend(block.dependencies);
                result.relative_depth = result.relative_depth.max(block.relative_depth);
                at += i64::from(count);
            }
            result.suppressed = merged_suppression(result.suppressed);
            Ok(result)
        })();
        self.active_frames -= reservation + graph.resident;
        result
    }

    fn read_shared_program<'plan>(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        graph: &mut Graph<'plan>,
        program: Program<'_, 'plan>,
        start: AudioSample,
        frames: u32,
        control: WorkControl<'_>,
    ) -> Result<WideBlock, StageAudioError> {
        control.check()?;
        if frames == 0 || frames > MAX_OUTPUT_FRAMES {
            return Err(StageAudioError::Range);
        }
        if program.depth > self.limits.maximum_depth {
            return Err(StageAudioError::Limit("nested stage depth"));
        }
        let end = AudioSample(
            start
                .0
                .checked_add(i64::from(frames))
                .ok_or(TimeError::Overflow)?,
        );
        let plan = graph.plan;
        let original_range = program.context.original.support(plan, start..end)?;
        let owners = if original_range.start < original_range.end {
            Some(
                program
                    .context
                    .original
                    .owners(plan, original_range.clone(), control)?,
            )
        } else {
            None
        };
        let sound_support = match &program.context.sounds {
            Sounds::Root => AudioSample(0)..plan.audio_duration()?,
            Sounds::Owner(context) => context.samples(),
            Sounds::None => start..start,
        };
        let sound_range = start.max(sound_support.start)..end.min(sound_support.end);
        let context = sound_context(&program.context.sounds);
        let retained = if sound_range.start < sound_range.end {
            self.prepare_sound_processing_plans_in_context(
                plan,
                sound_range.clone(),
                control,
                context,
            )?
        } else {
            BTreeMap::new()
        };
        let beats = if sound_range.start < sound_range.end {
            self.prepare_beat_sounds(plan, &retained, sound_range.clone(), control, context)?
        } else {
            Vec::new()
        };
        let root_range = start.max(AudioSample(0))..end.min(plan.audio_duration()?);
        let include_root = match &program.context.sounds {
            Sounds::Root => true,
            Sounds::Owner(context) => {
                context.owner().node == plan.metadata().root && context.owner().repeats.is_empty()
            }
            Sounds::None => false,
        };
        let roots = if include_root && root_range.start < root_range.end {
            self.prepare_root_sounds(plan, root_range.start, count(&root_range)?, control)?
        } else {
            Vec::new()
        };
        let mut mix = SharedMix::new(
            start,
            frames,
            program.stop.cloned(),
            program.output_voice,
            plan.has_audio_component_automation(),
            program.stop.is_none().then_some(graph.sends).flatten(),
            &graph.skip,
        );
        let original_id = graph.voice(Voice::Original)?;
        let original_routes = owners
            .as_ref()
            .map(|owners| {
                owners
                    .spans()
                    .iter()
                    .map(|span| {
                        if span.original_is_silent() {
                            Ok(None)
                        } else {
                            mix.route(
                                span.owners(),
                                span.samples(),
                                (original_id, Default::default()),
                                control,
                            )
                            .map(Some)
                        }
                    })
                    .collect::<Result<Vec<_>, StageAudioError>>()
            })
            .transpose()?
            .unwrap_or_default();
        let root_ids = roots
            .iter()
            .map(|root| graph.voice(Voice::Root(root.id.clone())))
            .collect::<Result<Vec<_>, _>>()?;
        let root_routes = if !roots.is_empty() {
            let root_owners =
                plan.audio_gain_owners(root_range.clone(), control.query_limits()?)?;
            control.spend_plan_work(root_owners.work())?;
            let root = root_owners
                .spans()
                .first()
                .and_then(|span| span.owners().first())
                .ok_or(PlanError::InvalidPlan("missing root mix clock"))?;
            if root.instance().node != plan.metadata().root {
                return Err(PlanError::InvalidPlan("invalid root mix clock").into());
            }
            root_ids
                .iter()
                .zip(&roots)
                .map(|(&id, sound)| {
                    mix.route(
                        std::slice::from_ref(root),
                        root_range.clone(),
                        (id, sound.voice.event().tail_sends),
                        control,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?
        } else {
            Vec::new()
        };
        let mut beat_routes = Vec::new();
        for beat in &beats {
            for occurrence in &beat.occurrences {
                let id = graph.voice(Voice::Beat(
                    beat.owner.clone(),
                    beat.id.clone(),
                    occurrence.voice.instance().clone(),
                ))?;
                beat_routes.push((
                    id,
                    mix.route(
                        occurrence.voice.gain_owners(),
                        occurrence.samples.clone(),
                        (id, beat.event.tail_sends),
                        control,
                    )?,
                ));
            }
        }
        let reservation = u64::from(frames)
            .checked_mul(6)
            .and_then(|n| n.checked_add(mix.resident_frames().ok()?))
            .ok_or(StageAudioError::Limit("shared EQ mix PCM"))?;
        self.make_room(reservation, false, control)?;
        self.active_frames += reservation;
        let result = (|| {
            mix.allocate();
            let mut dependencies = Dependencies::new();
            let mut relative_depth = 0;
            let admitted = |id| program.allowed.is_none_or(|mask| mask.contains(id));
            if let Some(owners) = owners {
                let raw = match &program.context.original {
                    Context::Root => self.read_controlled(
                        provider,
                        original_range.start,
                        count(&original_range)?,
                        control,
                        true,
                        program.depth,
                    )?,
                    Context::Domain(domain) => self.read_context_controlled(
                        provider,
                        domain,
                        original_range.start,
                        count(&original_range)?,
                        control,
                        (program.depth, true),
                    )?,
                    Context::Retained(input) => self.read_retained_controlled(
                        provider,
                        input,
                        original_range.start,
                        count(&original_range)?,
                        control,
                        (program.depth, false),
                    )?,
                };
                for (span, route) in owners.spans().iter().zip(original_routes) {
                    for sample in span.samples().start.0..span.samples().end.0 {
                        let value = raw.samples[(sample - original_range.start.0) as usize];
                        if span.support() == AudioOwnerSupport::Inactive && value != [0.0; 2] {
                            return Err(StageAudioError::Unsupported(
                                "nonzero Original outside owner support",
                            ));
                        }
                        if let Some(route) = &route {
                            let allow = admitted(original_id)
                                && span.support() == AudioOwnerSupport::Active
                                && permitted(&raw, AudioSample(sample));
                            mix.add(
                                route,
                                (sample - start.0) as usize,
                                value,
                                0,
                                allow.then_some(original_id),
                                control,
                            )?;
                        }
                    }
                }
                relative_depth = raw.relative_depth;
                dependencies.extend(raw.dependencies);
            }
            for ((root, id), route) in roots.into_iter().zip(root_ids).zip(root_routes) {
                let event = root.voice.event();
                let source =
                    resolve_source(provider, plan, &event.source.asset, control.cancelled)?;
                dependencies.insert(
                    event.source.asset.clone(),
                    control.observe(&event.source.asset, source)?,
                );
                let raw =
                    self.read_prepared_root_sound(provider, root.input, root_range.start, control)?;
                add_sound(
                    &mut mix,
                    &route,
                    &raw,
                    &root.fades,
                    event.gain_millidecibels,
                    admitted(id).then_some(id),
                    control,
                )?;
                relative_depth = relative_depth.max(raw.relative_depth);
                dependencies.extend(raw.dependencies);
            }
            let mut routes = beat_routes.into_iter();
            for beat in beats {
                for occurrence in beat.occurrences {
                    let (id, route) = routes
                        .next()
                        .ok_or(PlanError::InvalidPlan("missing beat sound route"))?;
                    let raw = self.read_prepared_beat_sound(
                        provider,
                        occurrence.input,
                        occurrence.samples,
                        control,
                    )?;
                    add_sound(
                        &mut mix,
                        &route,
                        &raw,
                        &occurrence.fades,
                        beat.event.gain_millidecibels,
                        admitted(id).then_some(id),
                        control,
                    )?;
                    relative_depth = relative_depth.max(raw.relative_depth);
                    dependencies.extend(raw.dependencies);
                }
                let asset = &beat.event.source.asset;
                let source = resolve_source(provider, plan, asset, control.cancelled)?;
                dependencies.insert(asset.clone(), control.observe(asset, source)?);
            }
            let mixed = mix.finish(control, |owner, stage, at, permissions, voice| {
                let id = self.shared_eq_node(graph, program, owner, at, stage, control)?;
                let tile =
                    at.0.div_euclid(OUTPUT as i64)
                        .checked_mul(OUTPUT as i64)
                        .ok_or(TimeError::Overflow)?;
                let block = self.shared_eq_tile(
                    provider,
                    graph,
                    id,
                    (tile, permissions, voice),
                    control,
                    program.depth + 1,
                )?;
                dependencies.extend(block.dependencies.clone());
                relative_depth = relative_depth.max(block.relative_depth);
                Ok(block.samples[(at.0 - tile) as usize])
            })?;
            if program
                .depth
                .checked_add(relative_depth)
                .is_none_or(|depth| depth > self.limits.maximum_depth)
            {
                return Err(StageAudioError::Limit("nested stage depth"));
            }
            Ok(WideBlock {
                samples: mixed.samples,
                suppressed: mixed.suppressed,
                dependencies,
                relative_depth,
            })
        })();
        self.active_frames -= reservation;
        result
    }

    fn shared_eq_node<'plan>(
        &self,
        graph: &mut Graph<'plan>,
        program: Program<'_, 'plan>,
        owner: &AudioOwnerClock<'plan>,
        at: AudioSample,
        stage: usize,
        control: WorkControl<'_>,
    ) -> Result<usize, StageAudioError> {
        if let Some(id) = graph.nodes.iter().position(|node| {
            node.parent == program.parent
                && node.boundary.before_stage() == stage
                && node.boundary.matches_clock_at(owner, at)
        }) {
            return Ok(id);
        }
        if graph.nodes.len() >= self.limits.maximum_prepared_stages as usize {
            return Err(StageAudioError::Limit("EQ owner count"));
        }
        let key = OwnerKey::new(owner);
        let sounds = if owner.kind() == deadpan_plan::AudioOwnerKind::Node {
            let context = graph
                .plan
                .audio_sound_instance_context(owner.instance(), control.query_limits()?)?;
            control.spend_plan_work(context.construction_work())?;
            Sounds::Owner(Arc::new(context))
        } else {
            Sounds::None
        };
        let range = program.context.original.support(
            graph.plan,
            at..AudioSample(at.0.checked_add(1).ok_or(TimeError::Overflow)?),
        )?;
        let observed = if range.start < range.end {
            let query = program
                .context
                .original
                .owners(graph.plan, range, control)?;
            query
                .spans()
                .iter()
                .flat_map(|span| span.owners())
                .find(|clock| OwnerKey::new(clock) == key)
                .cloned()
        } else {
            None
        };
        let original = if let Some(clock) = observed {
            program
                .context
                .original
                .owner_context(graph.plan, &clock, at, control)?
        } else if let Sounds::Owner(context) = &sounds {
            Context::Domain(Arc::new(context.processing_domain().clone()))
        } else {
            return Err(PlanError::InvalidPlan("missing shared EQ Original context").into());
        };
        let recipe = owner
            .treatments()
            .and_then(|recipe| recipe.equalizer())
            .ok_or(PlanError::InvalidPlan("EQ recipe absent"))?;
        control.spend_plan_work(deadpan_dsp::FINITE_FILTER_TAPS)?;
        let filter = crate::Equalizer::new(recipe, control.cancelled)?;
        control.check()?;
        graph.nodes.push(Node {
            parent: program.parent,
            context: Contexts { original, sounds },
            boundary: owner.treatment_boundary(stage)?,
            stop: Boundary {
                owner: key,
                before: stage,
            },
            filter: Some(filter),
            tiles: BTreeMap::new(),
        });
        Ok(graph.nodes.len() - 1)
    }

    fn shared_eq_tile(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        graph: &mut Graph<'_>,
        id: usize,
        key: (i64, Permissions, Option<usize>),
        control: WorkControl<'_>,
        depth: usize,
    ) -> Result<Arc<WideBlock>, StageAudioError> {
        control.check()?;
        if depth > self.limits.maximum_depth {
            return Err(StageAudioError::Limit("nested stage depth"));
        }
        if let Some(block) = graph.nodes[id].tiles.get(&key) {
            if depth
                .checked_sub(1)
                .and_then(|parent| parent.checked_add(block.relative_depth))
                .is_none_or(|value| value > self.limits.maximum_depth)
            {
                return Err(StageAudioError::Limit("nested stage depth"));
            }
            return Ok(Arc::clone(block));
        }
        let begin = key
            .0
            .checked_sub(RADIUS as i64)
            .ok_or(TimeError::Overflow)?;
        let end = begin.checked_add(INPUT as i64).ok_or(TimeError::Overflow)?;
        let context = graph.nodes[id].context.clone();
        let boundary = graph.nodes[id].stop.clone();
        let mut filter = graph.nodes[id]
            .filter
            .take()
            .ok_or(PlanError::InvalidPlan("recursive EQ cycle"))?;
        let reservation = ((INPUT + OUTPUT * 2) * 2) as u64;
        let result = (|| {
            self.reserve_eq_tile(reservation, control)?;
            let result = (|| {
                let mut left = vec![0.0; INPUT];
                let mut right = vec![0.0; INPUT];
                let mut dependencies = Dependencies::new();
                let mut relative_depth = 1;
                let mut at = begin;
                while at < end {
                    let count = u32::try_from((end - at).min(i64::from(MAX_OUTPUT_FRAMES)))
                        .map_err(|_| StageAudioError::Range)?;
                    let block = self.read_shared_program(
                        provider,
                        graph,
                        Program {
                            context: &context,
                            stop: Some(&boundary),
                            allowed: Some(key.1),
                            output_voice: key.2,
                            parent: id,
                            depth,
                        },
                        AudioSample(at),
                        count,
                        control,
                    )?;
                    for (index, frame) in block.samples.iter().enumerate() {
                        left[(at - begin) as usize + index] = frame[0];
                        right[(at - begin) as usize + index] = frame[1];
                    }
                    dependencies.extend(block.dependencies);
                    relative_depth = relative_depth.max(block.relative_depth + 1);
                    at += i64::from(count);
                }
                let mut out_left = vec![0.0; OUTPUT];
                let mut out_right = vec![0.0; OUTPUT];
                filter.process_tile(
                    &left,
                    &right,
                    &mut out_left,
                    &mut out_right,
                    control.cancelled,
                )?;
                let samples = out_left
                    .into_iter()
                    .zip(out_right)
                    .map(|(l, r)| [l, r])
                    .collect();
                control.check()?;
                let resident = (OUTPUT * 2) as u64;
                self.make_room(resident, false, control)?;
                self.active_frames += resident;
                graph.resident += resident;
                let block = Arc::new(WideBlock {
                    samples,
                    suppressed: Vec::new(),
                    dependencies,
                    relative_depth,
                });
                graph.nodes[id].tiles.insert(key, Arc::clone(&block));
                Ok(block)
            })();
            self.active_frames -= reservation;
            result
        })();
        graph.nodes[id].filter = Some(filter);
        result
    }
}

fn add_sound(
    mix: &mut SharedMix<'_>,
    route: &shared_mix::Route<'_>,
    raw: &ReadBlock,
    fades: &[deadpan_plan::AudioSoundGateSpan],
    gain: i32,
    id: Option<usize>,
    control: WorkControl<'_>,
) -> Result<(), StageAudioError> {
    let start = mix.start;
    let end = AudioSample(
        raw.start
            .0
            .checked_add(raw.samples.len() as i64)
            .ok_or(TimeError::Overflow)?,
    );
    sound_events::validate_gate_envelopes(fades, raw.start..end)?;
    for span in fades {
        for at in span.samples.start.0..span.samples.end.0 {
            control.check()?;
            let sample = raw.samples[(at - raw.start.0) as usize];
            let edge = crate::edges::edge_gain(
                span.length,
                i128::from(span.progress_at_start) + i128::from(at - span.samples.start.0),
                span.start_edge == deadpan_core::AudioEdgePolicy::Automatic,
                span.end_edge == deadpan_core::AudioEdgePolicy::Automatic,
            );
            let allowed = if span.length != 0 && permitted(raw, AudioSample(at)) {
                id
            } else {
                None
            };
            mix.add(
                route,
                (at - start.0) as usize,
                sample.map(|value| value * edge),
                gain,
                allowed,
                control,
            )?;
        }
    }
    Ok(())
}
