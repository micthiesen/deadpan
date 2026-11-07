//! Recursive shared temporal processing. Each EQ or wet tile evaluates the complete
//! ordered bus prefix with the consuming sample's allowed contribution set.
//! This keeps silence permissions distinct. Selected contribution reads retain
//! the common saturation drive from all permitted inputs.

use super::authored_eq::Context;
#[cfg(test)]
use super::shared_mix::MixedSuppression;
use super::shared_mix::{
    AdmittedStop, Boundary, Contribution, OwnerKey, Permissions, SharedMix, Temporal,
};
use super::*;

#[path = "shared_eq/node_capacity.rs"]
pub(super) mod node_capacity;
#[path = "shared_eq/node_description.rs"]
mod node_description;

#[cfg(test)]
#[path = "shared_eq/admission_tests.rs"]
mod admission_tests;
#[cfg(test)]
#[path = "shared_eq/caller_graph_tests.rs"]
mod caller_graph_tests;
#[cfg(test)]
#[path = "shared_eq/sound_admission_tests.rs"]
mod sound_admission_tests;
#[cfg(test)]
#[path = "shared_eq/wide_query_graph_tests.rs"]
mod wide_query_graph_tests;
use deadpan_core::{NodeId, SoundId};
use deadpan_dsp::{
    FINITE_FILTER_INPUT_FRAMES as INPUT, FINITE_FILTER_OUTPUT_FRAMES as OUTPUT,
    FINITE_FILTER_RADIUS as RADIUS,
};
use deadpan_plan::{
    AdmittedAudioTreatmentBoundary, AudioOwnerClock, AudioOwnerSupport, AudioSoundOwnerContext,
};

#[path = "event_returns.rs"]
mod event_returns;
use event_returns::{EventKey, EventRead, EventSource};
#[path = "shared_eq/original_inputs.rs"]
pub(super) mod original_inputs;

#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
#[path = "shared_eq/cached_prediction_baseline.rs"]
mod cached_prediction_baseline;
#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
#[path = "shared_eq/cached_prediction_tests.rs"]
mod cached_prediction_tests;
#[cfg(test)]
#[path = "shared_eq/cached_prediction_trace.rs"]
mod cached_prediction_trace;
#[path = "shared_eq/permission_families.rs"]
mod permission_families;
use permission_families::{FamilyRegistry, FamilyToken};

#[path = "shared_eq/policy_prediction.rs"]
mod policy_prediction;
pub(super) use policy_prediction::PopulationComplete;
#[cfg(test)]
pub(super) use policy_prediction::{
    Inputs as PopulationFixtureInputs, populate_inputs_for_test, populate_roots_for_test,
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
    suppressed: AudioSuppressionMask,
    dependencies: Dependencies,
    relative_depth: usize,
}

#[cfg(test)]
#[test]
fn opaque_mask_keeps_the_private_wide_block_shallow_layout() {
    struct ReconstructedPreEWideBlock {
        samples: Vec<[f64; 2]>,
        suppressed: Vec<Range<AudioSample>>,
        dependencies: Dependencies,
        relative_depth: usize,
    }
    let legacy = ReconstructedPreEWideBlock {
        samples: Vec::new(),
        suppressed: Vec::new(),
        dependencies: Dependencies::new(),
        relative_depth: 0,
    };
    let current = WideBlock {
        samples: Vec::new(),
        suppressed: AudioSuppressionMask::default(),
        dependencies: Dependencies::new(),
        relative_depth: 0,
    };
    assert_eq!(
        std::mem::size_of_val(&current),
        std::mem::size_of_val(&legacy)
    );
    assert_eq!(
        std::mem::align_of_val(&current),
        std::mem::align_of_val(&legacy)
    );
    let ReconstructedPreEWideBlock {
        samples,
        suppressed,
        dependencies,
        relative_depth,
    } = legacy;
    drop((samples, suppressed, dependencies, relative_depth));
}

struct Node<'plan, 'work> {
    parent: usize,
    context: Contexts<'plan>,
    boundary: AdmittedAudioTreatmentBoundary<
        'plan,
        sound_context_admission::Issuer<'work>,
        sound_context_admission::Lease<'work>,
    >,
    stop:
        AdmittedStop<sound_context_admission::Issuer<'work>, sound_context_admission::Lease<'work>>,
    filter: Option<crate::Equalizer>,
    kind: Temporal,
    tiles: BTreeMap<(i64, Permissions, Option<usize>), Arc<WideBlock>>,
}
struct Graph<'plan, 'work> {
    plan: &'plan RenderPlan,
    nodes: Vec<Node<'plan, 'work>>,
    resident: u64,
    node_capacity_frames: u64,
    wet_filters: [Option<crate::FiniteWetEffect>; 2],
    event_tiles: BTreeMap<EventKey, Arc<WideBlock>>,
    event_nodes: std::collections::BTreeSet<EventKey>,
    event_filters: BTreeMap<EventKey, crate::Equalizer>,
    voices: BTreeMap<Voice, usize>,
    permission_families: BTreeMap<InstancePath, BTreeMap<usize, Option<FamilyToken<'plan>>>>,
    selected_family_count: usize,
    canonical_families: FamilyRegistry<'plan>,
    sends: Option<deadpan_core::TailEffect>,
    skip: Vec<authored_gain::OwnerIdentity>,
    original_only: bool,
    original_inputs: original_inputs::Cache<'plan>,
}

#[derive(Default)]
pub(super) struct InputSelection<'a> {
    pub(super) depth: usize,
    pub(super) sends: Option<deadpan_core::TailEffect>,
    pub(super) skip: &'a [authored_gain::OwnerIdentity],
    pub(super) original_only: bool,
}
impl<'plan, 'work> Graph<'plan, 'work> {
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

/// Expand only the contribution identity used inside this temporal owner's
/// input. Raw sample permissions and delivery supports are still checked by
/// add_sound. Families cannot be inferred from a shared label or sound ID.
fn split_family_aliases(
    families: &[(usize, FamilyToken<'_>)],
    selected: Permissions,
    control: WorkControl<'_>,
) -> Result<Permissions, StageAudioError> {
    let mut aliases = Permissions::default();
    for (id, family) in families {
        if selected.contains(*id) {
            continue;
        }
        for (candidate, other) in families {
            if !selected.contains(*candidate) {
                continue;
            }
            control.spend_plan_work(1)?;
            if family.same(other) {
                aliases.insert(*id)?;
                break;
            }
        }
    }
    Ok(aliases)
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
        let cache_reservation = original_inputs::Cache::resident_frames()?;
        let reservation = u64::from(frames)
            .checked_mul(3)
            .and_then(|value| value.checked_add(cache_reservation))
            .ok_or(StageAudioError::Range)?;
        self.make_room(reservation, false, control)?;
        self.active_frames = self
            .active_frames
            .checked_add(reservation)
            .ok_or(StageAudioError::Range)?;
        let plan = Arc::clone(&self.plan);
        let mut graph = Graph {
            plan: &plan,
            nodes: Vec::new(),
            resident: 0,
            node_capacity_frames: 0,
            wet_filters: std::array::from_fn(|_| None),
            event_tiles: BTreeMap::new(),
            event_nodes: Default::default(),
            event_filters: BTreeMap::new(),
            voices: BTreeMap::new(),
            permission_families: BTreeMap::new(),
            selected_family_count: 0,
            canonical_families: FamilyRegistry::default(),
            sends: selection.sends,
            skip: selection.skip.to_vec(),
            original_only: selection.original_only,
            original_inputs: original_inputs::Cache::default(),
        };
        let result = (|| {
            let context = Contexts {
                original: Context::Root,
                sounds: if selection.original_only {
                    Sounds::None
                } else {
                    Sounds::Root
                },
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
                suppressed: AudioSuppressionMask::default(),
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
                result
                    .suppressed
                    .extend(block.suppressed, &mut self.masks(control))?;
                result.dependencies.extend(block.dependencies);
                result.relative_depth = result.relative_depth.max(block.relative_depth);
                at += i64::from(count);
            }
            result.suppressed = result.suppressed.merge(&self.masks(control))?;
            Ok(result)
        })();
        self.release_shared_graph(graph, reservation);
        result
    }

    fn release_shared_graph(&mut self, mut graph: Graph<'_, '_>, reservation: u64) {
        let graph_resident = graph.resident;
        graph.original_inputs.clear(self);
        drop(graph);
        self.active_frames -= reservation + graph_resident;
    }

    fn read_shared_program<'plan, 'work>(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        graph: &mut Graph<'plan, 'work>,
        program: Program<'_, 'plan>,
        start: AudioSample,
        frames: u32,
        control: WorkControl<'work>,
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
            Some({
                let mut admission = sound_context_admission::Admission::new(self, control);
                program.context.original.owners_admitted(
                    plan,
                    original_range.clone(),
                    control,
                    &mut admission,
                )?
            })
        } else {
            None
        };
        let sound_support = match &program.context.sounds {
            Sounds::Root => AudioSample(0)..plan.audio_duration()?,
            // Retained temporal input can extend beyond the newly rounded
            // nominal owner by its accumulated sample phase. Each checked
            // occurrence route admits its own exact preparation support.
            Sounds::Owner(_) => start..end,
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
            program.stop,
            program.output_voice,
            plan.has_audio_component_automation() || plan.has_audio_wet_sends(),
            program.stop.is_none().then_some(graph.sends).flatten(),
            &graph.skip,
        );
        let original_id = graph.voice(Voice::Original)?;
        let original_routes = owners
            .as_ref()
            .map(|owners| {
                owners
                    .query()
                    .spans()
                    .iter()
                    .map(|span| {
                        if span.original_is_silent() {
                            Ok(None)
                        } else {
                            mix.route(
                                span.owners(),
                                span.samples(),
                                (original_id, Contribution::Original),
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
            let root_owners = {
                let mut admission = sound_context_admission::Admission::new(self, control);
                Context::Root.owner_query_admitted(
                    plan,
                    root_range.clone(),
                    control,
                    &mut admission,
                )?
            };
            let root = root_owners
                .query()
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
                        (
                            id,
                            Contribution::Sound(sound.voice.event().treatments.tail_sends()),
                        ),
                        control,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?
        } else {
            Vec::new()
        };
        let mut beat_routes = Vec::new();
        let mut families = Vec::new();
        for beat in &beats {
            for occurrence in &beat.occurrences {
                let id = graph.voice(Voice::Beat(
                    beat.owner.clone(),
                    beat.id.clone(),
                    occurrence.voice.instance().clone(),
                ))?;
                if let Some(family) = &occurrence.permission_family {
                    families.push((id, self.intern_permission_family(graph, family, control)?));
                }
                beat_routes.push((
                    id,
                    mix.route(
                        occurrence.voice.gain_owners(),
                        occurrence.samples.clone(),
                        (id, Contribution::Sound(beat.event.treatments.tail_sends())),
                        control,
                    )?,
                ));
            }
        }
        if let Some(context) = context {
            let mut selected = program.allowed.unwrap_or_default();
            if let Some(output) = program.output_voice {
                selected.insert(output)?;
            }
            families.extend(self.selected_split_families(graph, context, selected, control)?);
        }
        let allowed_aliases = program
            .allowed
            .map(|selected| split_family_aliases(&families, selected, control))
            .transpose()?
            .unwrap_or_default();
        if let Some(output) = program.output_voice {
            let mut selected = Permissions::default();
            selected.insert(output)?;
            mix.include_output_aliases(split_family_aliases(&families, selected, control)?);
        }
        let admitted = |id| {
            program.allowed.is_none_or(|mask| mask.contains(id)) || allowed_aliases.contains(id)
        };
        let predict = mix.uses_components()
            && policy_prediction::available(&program.context.original, &roots, &beats, control)?;
        if predict {
            // Routes have not allocated policy or PCM. This only installs an
            // empty prediction carrier; the fallible query factories do not
            // inspect SharedMix. Its complete payload is still admitted below.
            mix.begin_prediction()?;
        }
        let reservation = u64::from(frames)
            .checked_mul(12)
            .and_then(|n| n.checked_add(mix.resident_frames().ok()?))
            .ok_or(StageAudioError::Limit("shared EQ mix PCM"));
        let mut cached_original = None;
        let mut prepared_original = if predict && owners.is_some() && admitted(original_id) {
            let count = count(&original_range)?;
            #[cfg(test)]
            let enabled = !cached_prediction_trace::legacy();
            #[cfg(not(test))]
            let enabled = true;
            if enabled && let Ok(additional) = &reservation {
                #[cfg(test)]
                let _probe_phase = cached_prediction_trace::ScopeGuard::new(
                    cached_prediction_trace::Phase::Probe,
                    control,
                );
                cached_original = self.try_cached_original_for_prediction(
                    provider,
                    graph,
                    &program.context.original,
                    (original_range.clone(), *additional),
                    control,
                    program.depth,
                )?;
            }
            if cached_original.is_none() {
                #[cfg(test)]
                let _queries_phase = cached_prediction_trace::ScopeGuard::new(
                    cached_prediction_trace::Phase::Queries,
                    control,
                );
                Some(match &program.context.original {
                    Context::Root => self.prepare_original_root(
                        plan,
                        original_range.start,
                        count,
                        control,
                        true,
                    )?,
                    Context::Domain(domain) => self.prepare_original_domain(
                        domain,
                        original_range.start,
                        count,
                        control,
                        true,
                    )?,
                    Context::Retained(_) => {
                        return Err(StageAudioError::Unsupported(
                            "retained Original policy prediction",
                        ));
                    }
                })
            } else {
                None
            }
        } else {
            None
        };
        // Preserve the ordinary cold query-failure priority over admission.
        let mut reservation = reservation?;
        self.make_room(reservation, false, control)?;
        self.active_frames += reservation;
        let result = (|| {
            if predict {
                mix.allocate_policy();
                let original = match (&owners, &prepared_original, &cached_original) {
                    (Some(owners), _, Some(block)) => Some(policy_prediction::Original {
                        policy: policy_prediction::OriginalPolicy::Cached(block),
                        owners: owners.query(),
                        routes: &original_routes,
                        id: original_id,
                    }),
                    (Some(owners), Some(request), None) => Some(policy_prediction::Original {
                        policy: policy_prediction::OriginalPolicy::Queries(request.queries()),
                        owners: owners.query(),
                        routes: &original_routes,
                        id: original_id,
                    }),
                    (None, _, _) => None,
                    (Some(_), None, None) if !admitted(original_id) => None,
                    _ => {
                        return Err(
                            PlanError::InvalidPlan("Original prediction request absent").into()
                        );
                    }
                };
                #[cfg(test)]
                let _prediction_phase = cached_prediction_trace::ScopeGuard::new(
                    cached_prediction_trace::Phase::Prediction,
                    control,
                );
                policy_prediction::populate(
                    &mut mix,
                    policy_prediction::Inputs {
                        original,
                        roots: &roots,
                        root_range: root_range.clone(),
                        root_ids: &root_ids,
                        root_routes: &root_routes,
                        beats: &beats,
                        beat_routes: &beat_routes,
                    },
                    admitted,
                    control,
                )?;
            }
            mix.allocate();
            let mut dependencies = Dependencies::new();
            let mut relative_depth = 0;
            if let Some(owners) = owners {
                #[cfg(test)]
                let original_read_phase = cached_prediction_trace::ScopeGuard::new(
                    cached_prediction_trace::Phase::OriginalRead,
                    control,
                );
                let raw = if let Some(block) = cached_original.take() {
                    original_inputs::Read::Cached(block)
                } else if let Some(request) = prepared_original.take() {
                    self.shared_original_input_prepared(
                        provider,
                        graph,
                        &program.context.original,
                        (original_range.clone(), request),
                        control,
                        program.depth,
                    )?
                } else {
                    self.shared_original_input(
                        provider,
                        graph,
                        &program.context.original,
                        original_range.clone(),
                        control,
                        program.depth,
                    )?
                };
                #[cfg(test)]
                drop(original_read_phase);
                for (span, route) in owners.query().spans().iter().zip(original_routes) {
                    for sample in span.samples().start.0..span.samples().end.0 {
                        let value = raw.samples[(sample - original_range.start.0) as usize];
                        if span.support() == AudioOwnerSupport::Inactive && value != [0.0; 2] {
                            return Err(StageAudioError::Unsupported(
                                "nonzero Original outside owner support",
                            ));
                        }
                        if let Some(route) = &route {
                            let allow = policy_prediction::original_allowed(
                                admitted(original_id),
                                span.support(),
                                || permitted(&raw, AudioSample(sample)),
                            );
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
                dependencies.extend(
                    raw.dependencies
                        .iter()
                        .map(|(id, fingerprint)| (id.clone(), *fingerprint)),
                );
                // Later sound returns can need the same memory. The cache
                // owns an evictable copy; do not pin it through those reads.
                drop(raw);
            }
            for ((root, id), route) in roots.into_iter().zip(root_ids).zip(root_routes) {
                let event = root.voice.event();
                let source =
                    resolve_source(provider, plan, &event.source.asset, control.cancelled)?;
                dependencies.insert(
                    event.source.asset.clone(),
                    control.observe(&event.source.asset, source)?,
                );
                let returned = if !event_returns::needs_processing(&event.treatments) {
                    None
                } else {
                    let input = deadpan_plan::AudioRoutedRootInput::Source(Box::new(
                        root.voice.processing_source()?,
                    ));
                    Some(self.read_event_returns(
                        provider,
                        graph,
                        EventSource {
                            input: &input,
                            route: root.voice.routed_input().map(|routed| routed.route()),
                        },
                        EventRead {
                            parent: program.parent,
                            voice: id,
                            start: root_range.start,
                            frames: count(&root_range)?,
                            treatments: &event.treatments,
                            edges: root.voice.event_edges(),
                            depth: program.depth,
                        },
                        control,
                    )?)
                };
                if let Some(returned) = &returned {
                    dependencies.extend(returned.returned.dependencies.clone());
                    relative_depth = relative_depth.max(returned.returned.relative_depth);
                }
                let raw =
                    self.read_prepared_root_sound(provider, root.input, root_range.start, control)?;
                add_sound(
                    &mut mix,
                    &route,
                    (&raw, returned.as_ref()),
                    (&root.fades, &root.post_fades),
                    event.treatments.trim().millidecibels(),
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
                    let returned = if !event_returns::needs_processing(&beat.event.treatments) {
                        None
                    } else {
                        let request = EventRead {
                            parent: program.parent,
                            voice: id,
                            start: occurrence.samples.start,
                            frames: count(&occurrence.samples)?,
                            treatments: &beat.event.treatments,
                            edges: occurrence
                                .voice
                                .event_edges((beat.event.start_edge, beat.event.end_edge)),
                            depth: program.depth,
                        };
                        Some(match &occurrence.input {
                            beat_sounds::PreparedBeatInput::Current(_) => {
                                let input = deadpan_plan::AudioRoutedRootInput::Occurrence(
                                    Box::new(occurrence.voice.clone()),
                                );
                                self.read_event_returns(
                                    provider,
                                    graph,
                                    EventSource {
                                        input: &input,
                                        route: None,
                                    },
                                    request,
                                    control,
                                )?
                            }
                            beat_sounds::PreparedBeatInput::Routed { plan, query } => self
                                .with_plan_scope(Arc::clone(plan), |reader| {
                                    reader.read_event_returns(
                                        provider,
                                        graph,
                                        EventSource {
                                            input: query.domain.input(),
                                            route: Some(query.domain.route()),
                                        },
                                        request,
                                        control,
                                    )
                                })?,
                        })
                    };
                    if let Some(returned) = &returned {
                        dependencies.extend(returned.returned.dependencies.clone());
                        relative_depth = relative_depth.max(returned.returned.relative_depth);
                    }
                    let raw = self.read_prepared_beat_sound(
                        provider,
                        occurrence.input,
                        occurrence.samples,
                        control,
                    )?;
                    add_sound(
                        &mut mix,
                        &route,
                        (&raw, returned.as_ref()),
                        (&occurrence.fades, &occurrence.post_fades),
                        beat.event.treatments.trim().millidecibels(),
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
            let released = mix.verify_prediction(control)?;
            let next_active = self
                .active_frames
                .checked_sub(released)
                .ok_or(StageAudioError::Range)?;
            let next_reservation = reservation
                .checked_sub(released)
                .ok_or(StageAudioError::Range)?;
            self.active_frames = next_active;
            reservation = next_reservation;
            let component_mask_frames = mix.component_mask_frames();
            #[cfg(test)]
            let component_mask_frames =
                component_mask_frames.filter(|_| !super::components_mask_request::legacy());
            #[cfg(test)]
            let components_receipt = component_mask_frames.and_then(|frames| {
                if !super::components_mask_trace::is_enabled() {
                    return None;
                }
                let work = control.work.borrow();
                super::components_mask_trace::begin_request(super::components_mask_trace::Request {
                    start: mix.start.0,
                    program_depth: program.depth,
                    frames,
                    active_frames: self.active_frames,
                    source_reservation_frames: reservation,
                    account_live_frames: self.owned_suppression.live_frames(),
                    plan_work_before: work.plan_work,
                    cache_work_before: work.cache_plan_work,
                })
            });
            let component_marks_result = if let Some(frames) = component_mask_frames {
                shared_components::ComponentMarks::new(frames, control).map(Some)
            } else {
                Ok(None)
            };
            #[cfg(test)]
            if let Some(receipt) = components_receipt {
                let work = control.work.borrow();
                super::components_mask_trace::source_initialized(
                    receipt,
                    &component_marks_result,
                    work.plan_work,
                    work.cache_plan_work,
                );
            }
            let component_marks = match component_marks_result {
                Ok(flags) => flags,
                Err(error) => {
                    #[cfg(test)]
                    if let Some(receipt) = components_receipt {
                        super::components_mask_trace::source_init_failure_complete(
                            receipt,
                            &error,
                            self.active_frames,
                            self.owned_suppression.live_frames(),
                            control,
                        );
                    }
                    return Err(error);
                }
            };
            let mixed_result = mix.finish_with_component_marks(
                control,
                component_marks,
                |owner, stage, at, permissions, voice| {
                    #[cfg(test)]
                    super::components_mask_trace::callback();
                    let id = self.shared_eq_node(graph, program, owner, at, stage, control)?;
                    let width = stage.output_frames();
                    let tile =
                        at.0.div_euclid(width as i64)
                            .checked_mul(width as i64)
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
                },
            );
            let mixed = match mixed_result {
                Ok(mixed) => mixed,
                Err(error) => {
                    #[cfg(test)]
                    if let Some(receipt) = components_receipt {
                        super::components_mask_trace::source_account_after(
                            self.owned_suppression.live_frames(),
                        );
                        super::components_mask_trace::complete_failure(
                            receipt,
                            &error,
                            self.active_frames,
                            self.owned_suppression.live_frames(),
                            control,
                        );
                    }
                    return Err(error);
                }
            };
            if program
                .depth
                .checked_add(relative_depth)
                .is_none_or(|depth| depth > self.limits.maximum_depth)
            {
                #[cfg(test)]
                if let Some(receipt) = components_receipt
                    && let MixedSuppression::PendingMarks { marks, .. } = &mixed.suppressed
                {
                    super::components_mask_trace::inline_source_snapshot(marks);
                    drop(mixed.suppressed);
                    super::components_mask_trace::inline_source_closed();
                    super::components_mask_trace::source_account_after(
                        self.owned_suppression.live_frames(),
                    );
                    let error = StageAudioError::Limit("nested stage depth");
                    super::components_mask_trace::complete_failure(
                        receipt,
                        &error,
                        self.active_frames,
                        self.owned_suppression.live_frames(),
                        control,
                    );
                    return Err(error);
                }
                return Err(StageAudioError::Limit("nested stage depth"));
            }
            // Keep the moved permission grid funded until projection drops it.
            #[cfg(test)]
            let pending_source = match &mixed.suppressed {
                MixedSuppression::Pending { start, permissions } => Some(
                    super::shared_mix_mask_trace::pending_source(*start, permissions),
                ),
                MixedSuppression::Ordinary(_) | MixedSuppression::PendingMarks { .. } => None,
            };
            #[cfg(test)]
            let permission_receipt = pending_source.and_then(|source| {
                super::shared_mix_mask_trace::begin(
                    source,
                    reservation,
                    self.active_frames,
                    self.owned_suppression.live_frames(),
                    control,
                )
            });
            #[cfg(test)]
            if components_receipt.is_some() {
                super::components_mask_trace::projection_begin();
            }
            let projection = {
                #[cfg(test)]
                {
                    mixed
                        .suppressed
                        .finish(&mut self.masks(control), permission_receipt)
                }
                #[cfg(not(test))]
                {
                    mixed.suppressed.finish(&mut self.masks(control))
                }
            };
            #[cfg(test)]
            if let Some(receipt) = components_receipt {
                super::components_mask_trace::projection_end();
                super::components_mask_trace::source_account_after(
                    self.owned_suppression.live_frames(),
                );
                super::components_mask_trace::complete(
                    receipt,
                    &projection,
                    self.active_frames,
                    self.owned_suppression.live_frames(),
                    control,
                );
            }
            #[cfg(test)]
            if let Some(receipt) = permission_receipt {
                super::shared_mix_mask_trace::complete(
                    receipt,
                    &projection,
                    self.active_frames,
                    self.owned_suppression.live_frames(),
                    control,
                );
            }
            let suppressed = projection?;
            Ok(WideBlock {
                samples: mixed.samples,
                suppressed,
                dependencies,
                relative_depth,
            })
        })();
        self.active_frames -= reservation;
        result
    }

    fn shared_eq_node<'plan, 'work>(
        &mut self,
        graph: &mut Graph<'plan, 'work>,
        program: Program<'_, 'plan>,
        owner: &AudioOwnerClock<'plan>,
        at: AudioSample,
        kind: Temporal,
        control: WorkControl<'work>,
    ) -> Result<usize, StageAudioError> {
        let stage = kind.boundary(owner);
        if let Some(id) = graph.nodes.iter().position(|node| {
            node.parent == program.parent
                && node.kind == kind
                && node.boundary.before_stage() == stage
                && node.boundary.matches_clock_at(owner, at)
        }) {
            return Ok(id);
        }
        if graph.nodes.len() + graph.event_nodes.len()
            >= self.limits.maximum_prepared_stages as usize
        {
            return Err(StageAudioError::Limit("temporal owner count"));
        }
        let mut description = {
            let mut admission = sound_context_admission::Admission::new(self, control);
            node_description::describe_admitted(
                graph.plan,
                node_description::Request {
                    incoming: program.context,
                    owner,
                    at,
                    kind,
                    stage,
                    original_only: graph.original_only,
                },
                control,
                &mut admission,
            )?
        };
        let sound_frames = description
            .sound
            .as_ref()
            .and_then(|value| value.lease())
            .map_or(0, sound_context_admission::Lease::frames);
        let original_frames = description
            .original
            .as_ref()
            .map(|value| {
                value
                    .leases()
                    .into_iter()
                    .flatten()
                    .try_fold(0_u64, |total, lease| {
                        total
                            .checked_add(lease.frames())
                            .ok_or(StageAudioError::Range)
                    })
            })
            .transpose()?
            .unwrap_or(0);
        let key_frames = description
            .stop
            .lease()
            .map_or(0, sound_context_admission::Lease::frames);
        let boundary_frames = description
            .boundary
            .lease()
            .map_or(0, sound_context_admission::Lease::frames);
        let metadata_frames = [original_frames, sound_frames, key_frames, boundary_frames]
            .into_iter()
            .try_fold(0_u64, |total, frames| {
                total.checked_add(frames).ok_or(StageAudioError::Range)
            })?;
        let expected_issuer = sound_context_admission::issuer(control);
        if description
            .sound
            .as_ref()
            .is_some_and(|value| value.issuer() != expected_issuer)
            || description
                .original
                .as_ref()
                .is_some_and(|value| value.issuer() != expected_issuer)
            || description.stop.issuer() != expected_issuer
            || description.boundary.issuer() != expected_issuer
        {
            return Err(PlanError::AudioMetadataIssuerMismatch.into());
        }
        let resident = match kind {
            Temporal::Equalizer(_) => u64::try_from(crate::Equalizer::preparation_bytes_bound())
                .map_err(|_| StageAudioError::Range)?
                .div_ceil(std::mem::size_of::<[f32; 2]>() as u64),
            Temporal::Wet(_) => 0,
        };
        let recipe = match kind {
            Temporal::Equalizer(_) => Some(
                owner
                    .treatments()
                    .and_then(|value| value.equalizer())
                    .ok_or(PlanError::InvalidPlan("EQ recipe absent"))?,
            ),
            Temporal::Wet(_) => None,
        };
        if recipe.is_some() {
            control.spend_plan_work(deadpan_dsp::FINITE_FILTER_TAPS)?;
        }
        // This reservation precedes filter construction: its make_room must
        // see the complete planned metadata plus the still-live old buffer.
        let maximum_nodes = self.limits.maximum_prepared_stages as usize;
        let mut capacity = node_capacity::prepare(
            self,
            &graph.nodes,
            graph.node_capacity_frames,
            maximum_nodes,
            control,
        )?;
        let retained = resident
            .checked_add(metadata_frames)
            .and_then(|value| value.checked_add(capacity.new_frames()))
            .ok_or(StageAudioError::Range)?;
        let next_active = self
            .active_frames
            .checked_sub(capacity.retired_frames())
            .and_then(|value| value.checked_add(retained))
            .ok_or(StageAudioError::Range)?;
        let next_resident = graph
            .resident
            .checked_sub(capacity.retired_frames())
            .and_then(|value| value.checked_add(retained))
            .ok_or(StageAudioError::Range)?;
        let next_capacity_frames = capacity.capacity_frames_after();
        let filter = if let Some(recipe) = recipe {
            self.make_room(resident, false, control)?;
            Some(crate::Equalizer::new(recipe, control.cancelled)?)
        } else {
            None
        };
        control.check()?;
        capacity.allocate()?;
        capacity.prepay_moves(control)?;
        capacity.validate_current(&graph.nodes, graph.node_capacity_frames)?;
        control.check()?;
        let node_description::Description {
            values,
            original,
            sound,
        } = &mut description;
        let context_transfer = {
            let mut admission = sound_context_admission::Admission::new(self, control);
            deadpan_plan::preflight_context_transfer(
                original.as_mut(),
                sound.as_mut(),
                &mut admission,
            )?
        };
        let [plan, core, buffer, sound] = context_transfer.leases();
        let unpublished = values.as_ref().expect("unpublished description");
        let transfer = sound_context_admission::preflight_transfer(
            control,
            sound_context_admission::TransferLeases {
                original: [plan, core, buffer],
                sound,
                key: unpublished.stop.lease(),
                boundary: unpublished.boundary.lease(),
                capacity: capacity.lease(),
            },
        )?;
        let id = graph.nodes.len();
        context_transfer.commit(|plan, core, buffer, sound| {
            let node_description::DescriptionValues {
                context,
                boundary,
                stop,
                kind,
            } = values.take().expect("unpublished description");
            capacity.publish(
                &mut graph.nodes,
                Node {
                    parent: program.parent,
                    context,
                    boundary,
                    stop,
                    filter,
                    kind,
                    tiles: BTreeMap::new(),
                },
            );
            // publish has physically freed the old vector before retiring it.
            self.active_frames = next_active;
            graph.resident = next_resident;
            graph.node_capacity_frames = next_capacity_frames;
            let Node { boundary, stop, .. } = &mut graph.nodes[id];
            boundary.with_lease_mut(|boundary| {
                stop.with_lease_mut(|key| {
                    transfer.commit(sound_context_admission::TransferLeasesMut {
                        original: [plan, core, buffer],
                        sound,
                        key,
                        boundary,
                        capacity: capacity.lease_mut(),
                    });
                });
            });
        });

        Ok(id)
    }

    fn shared_eq_tile<'plan, 'work>(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        graph: &mut Graph<'plan, 'work>,
        id: usize,
        key: (i64, Permissions, Option<usize>),
        control: WorkControl<'work>,
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
        let kind = graph.nodes[id].kind;
        let (history, input, output) = match kind {
            Temporal::Equalizer(_) => (RADIUS, INPUT, OUTPUT),
            Temporal::Wet(effect) => {
                let shape = crate::wet_effect_descriptor(effect);
                (
                    shape.history_frames,
                    shape.history_frames + shape.output_frames,
                    shape.output_frames,
                )
            }
        };
        let begin = key
            .0
            .checked_sub(history as i64)
            .ok_or(TimeError::Overflow)?;
        let end = begin.checked_add(input as i64).ok_or(TimeError::Overflow)?;
        let context = graph.nodes[id].context.clone();
        let boundary = {
            let mut admission = sound_context_admission::Admission::new(self, control);
            AdmittedStop::from_boundary(&graph.nodes[id].stop, &mut admission)?
        };
        let mut filter = match kind {
            Temporal::Equalizer(_) => Some(
                graph.nodes[id]
                    .filter
                    .take()
                    .ok_or(PlanError::InvalidPlan("recursive EQ cycle"))?,
            ),
            Temporal::Wet(_) => None,
        };
        let reservation = ((input + output * 2) * 2) as u64;
        let result = (|| {
            if input as u32 > self.limits.maximum_input_frames
                || output as u32 > self.limits.maximum_output_frames
            {
                return Err(StageAudioError::Limit("temporal input/output frames"));
            }
            {
                let mut work = control.work.borrow_mut();
                work.prepared_frames = work
                    .prepared_frames
                    .checked_add((input + output) as u64)
                    .ok_or(StageAudioError::Limit("prepared frames per read"))?;
                if work.prepared_frames > self.limits.maximum_prepared_frames {
                    return Err(StageAudioError::Limit("prepared frames per read"));
                }
            }
            self.make_room(reservation, false, control)?;
            self.active_frames += reservation;
            let result = (|| {
                let mut left = vec![0.0; input];
                let mut right = vec![0.0; input];
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
                let mut out_left = vec![0.0; output];
                let mut out_right = vec![0.0; output];
                if let Some(filter) = &mut filter {
                    filter.process_tile(
                        &left,
                        &right,
                        &mut out_left,
                        &mut out_right,
                        control.cancelled,
                    )?;
                } else if let Temporal::Wet(effect) = kind {
                    // Stateless kernels share one native workspace per effect
                    // in this request, including nested owners. Acquire only
                    // after recursive input preparation has finished.
                    self.process_shared_wet(
                        graph,
                        effect,
                        (&left, &right),
                        (&mut out_left, &mut out_right),
                        control,
                    )?;
                }
                let samples = out_left
                    .into_iter()
                    .zip(out_right)
                    .map(|(l, r)| [l, r])
                    .collect();
                control.check()?;
                let resident = (output * 2) as u64;
                self.make_room(resident, false, control)?;
                self.active_frames += resident;
                graph.resident += resident;
                let block = Arc::new(WideBlock {
                    samples,
                    suppressed: AudioSuppressionMask::default(),
                    dependencies,
                    relative_depth,
                });
                graph.nodes[id].tiles.insert(key, Arc::clone(&block));
                Ok(block)
            })();
            self.active_frames -= reservation;
            result
        })();
        graph.nodes[id].filter = filter;
        result
    }
}

/// Only processed event output consumes the second gate envelope. Keep this
/// decision beside its consumer and use the same canonical event predicate.
pub(super) fn prepare_post_gates(
    treatments: &deadpan_core::AudioTreatments,
    control: WorkControl<'_>,
    build: impl FnOnce(AudioQueryLimits) -> Result<deadpan_plan::AudioSoundGateQuery, PlanError>,
) -> Result<Vec<deadpan_plan::AudioSoundGateSpan>, StageAudioError> {
    control.spend_plan_work(1)?;
    if !event_returns::needs_processing(treatments) {
        return Ok(Vec::new());
    }
    let query = build(control.query_limits()?)?;
    control.spend_plan_work(query.work)?;
    Ok(query.spans)
}

fn add_sound(
    mix: &mut SharedMix<'_, '_>,
    route: &shared_mix::Route<'_>,
    (raw, returned): (&ReadBlock, Option<&event_returns::EventOutput>),
    (fades, post_fades): (
        &[deadpan_plan::AudioSoundGateSpan],
        &[deadpan_plan::AudioSoundGateSpan],
    ),
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
    if returned.is_some() {
        sound_events::validate_gate_envelopes(post_fades, raw.start..end)?;
    }
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
            let allowed = policy_prediction::sound_allowed(id, span.length, || {
                permitted(raw, AudioSample(at))
            });
            let value = if let Some(processed) = returned {
                let index = post_fades.partition_point(|span| span.samples.end.0 <= at);
                let span = post_fades
                    .get(index)
                    .filter(|span| span.samples.contains(&AudioSample(at)))
                    .ok_or(PlanError::InvalidPlan("event output gate absent"))?;
                let gate = crate::edges::edge_gain(
                    span.length,
                    i128::from(span.progress_at_start) + i128::from(at - span.samples.start.0),
                    span.start_edge == deadpan_core::AudioEdgePolicy::Automatic,
                    span.end_edge == deadpan_core::AudioEdgePolicy::Automatic,
                );
                processed.dry[(at - raw.start.0) as usize].map(|value| value * f64::from(gate))
            } else {
                sample.map(|value| f64::from(value * edge))
            };
            let wet = returned.map_or([0.0; 2], |returned| {
                returned.returned.samples[(at - raw.start.0) as usize]
                    .map(|value| value * f64::from(edge))
            });
            mix.add_wide(
                route,
                (at - start.0) as usize,
                (value, wet),
                if returned.is_some() { 0 } else { gain },
                allowed,
                control,
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn with_cached_original_for_point_test<P: AudioSourceProvider, R>(
    renderer: &mut StageAudio,
    provider: &mut P,
    control: WorkControl<'_>,
    action: impl FnOnce(&mut StageAudio, &mut P) -> R,
) -> R {
    let plan = Arc::clone(&renderer.plan);
    let mut graph = Graph {
        plan: &plan,
        nodes: Vec::new(),
        resident: 0,
        node_capacity_frames: 0,
        wet_filters: std::array::from_fn(|_| None),
        event_tiles: BTreeMap::new(),
        event_nodes: Default::default(),
        event_filters: BTreeMap::new(),
        voices: BTreeMap::new(),
        permission_families: BTreeMap::new(),
        selected_family_count: 0,
        canonical_families: FamilyRegistry::default(),
        sends: None,
        skip: Vec::new(),
        original_only: true,
        original_inputs: original_inputs::Cache::default(),
    };
    let cached = renderer
        .shared_original_input(
            provider,
            &mut graph,
            &Context::Root,
            AudioSample(0)..AudioSample(i64::from(MAX_OUTPUT_FRAMES)),
            control,
            0,
        )
        .unwrap();
    assert!(matches!(&cached, original_inputs::Read::Cached(_)));
    drop(cached);
    let result = action(renderer, provider);
    graph.original_inputs.clear(renderer);
    result
}
