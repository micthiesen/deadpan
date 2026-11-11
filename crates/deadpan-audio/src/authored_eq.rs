//! Recursive ordered EQ over Original owners. One request owns all filtered
//! tiles, source dependencies and resource admission; immutable contexts cannot
//! be reconstructed from node IDs or nominal automation coordinates alone.

#[cfg(test)]
#[path = "authored_eq/wide_query_tests.rs"]
mod wide_query_tests;

#[cfg(test)]
#[path = "authored_eq/legacy_node_baseline.rs"]
mod legacy_node_baseline;

use deadpan_core::AudioTreatmentStage;
use deadpan_dsp::{
    FINITE_FILTER_INPUT_FRAMES as INPUT, FINITE_FILTER_OUTPUT_FRAMES as OUTPUT,
    FINITE_FILTER_RADIUS as RADIUS,
};
use deadpan_plan::{AudioOwnerClock, AudioOwnerQuery, AudioOwnerSupport};

use super::*;

#[cfg(test)]
#[path = "authored_eq/node_admission_tests.rs"]
mod node_admission_tests;
#[path = "authored_eq/node_description.rs"]
mod node_description;
#[path = "authored_eq/span_start.rs"]
mod span_start;
#[cfg(test)]
#[path = "authored_eq/span_start_tests.rs"]
mod span_start_tests;
#[cfg(test)]
#[path = "authored_eq/tile_admission_tests.rs"]
mod tile_admission_tests;
#[path = "authored_eq/tile_block.rs"]
mod tile_block;
#[path = "authored_eq/tile_index.rs"]
mod tile_index;

#[cfg(test)]
#[path = "authored_eq/admission_tests.rs"]
mod admission_tests;
#[cfg(test)]
#[path = "eq_admission/fixture.rs"]
pub(super) mod eq_admission_fixture;

#[derive(Clone)]
pub(super) enum Context<'plan> {
    Root,
    Domain(Arc<AudioDomain<'plan>>),
}

impl<'plan> Context<'plan> {
    pub(super) fn support(
        &self,
        plan: &RenderPlan,
        range: Range<AudioSample>,
    ) -> Result<Range<AudioSample>, StageAudioError> {
        let support = match self {
            Self::Root => AudioSample(0)..plan.audio_duration()?,
            Self::Domain(domain) => domain.root_samples(),
        };
        Ok(range.start.max(support.start)..range.end.min(support.end))
    }

    /// Retain the actual guarded query through its renderer use scope.
    pub(super) fn owners_admitted<
        A: deadpan_plan::AudioSoundContextAdmission<Error = StageAudioError>,
    >(
        &self,
        plan: &'plan RenderPlan,
        range: Range<AudioSample>,
        control: WorkControl<'_>,
        admission: &mut A,
    ) -> Result<deadpan_plan::AdmittedAudioOwnerQuery<'plan, A::Issuer, A::Lease>, StageAudioError>
    {
        let query = self.owner_query_admitted(plan, range, control, admission)?;
        Self::validate_owner_query_layers(query.query())?;
        Ok(query)
    }

    /// Pay the exact query once. Most callers then validate every layer via
    /// owners_admitted. Root mix routing instead selects its checked root
    /// first and validates only that route, preserving its established order.
    pub(super) fn owner_query_admitted<
        A: deadpan_plan::AudioSoundContextAdmission<Error = StageAudioError>,
    >(
        &self,
        plan: &'plan RenderPlan,
        range: Range<AudioSample>,
        control: WorkControl<'_>,
        admission: &mut A,
    ) -> Result<deadpan_plan::AdmittedAudioOwnerQuery<'plan, A::Issuer, A::Lease>, StageAudioError>
    {
        let limits = control.query_limits()?;
        let query = match self {
            Self::Root => plan.audio_gain_owners_admitted(range, limits, admission)?,
            Self::Domain(domain) => domain.gain_owners_admitted(range, limits, admission)?,
        };
        if query.was_forwarded() != A::METERED {
            return Err(PlanError::InvalidPlan("wide owner query accounting mode differs").into());
        }
        if !query.was_forwarded() {
            control.spend_plan_work(query.work())?;
        }
        Ok(query)
    }

    #[cfg(test)]
    pub(super) fn owners(
        &self,
        plan: &'plan RenderPlan,
        range: Range<AudioSample>,
        control: WorkControl<'_>,
    ) -> Result<AudioOwnerQuery<'plan>, StageAudioError> {
        let query = match self {
            Self::Root => plan.audio_gain_owners(range, control.query_limits()?)?,
            Self::Domain(domain) => domain.gain_owners(range, control.query_limits()?)?,
        };
        Self::admit_owner_query(&query, control)?;
        Ok(query)
    }

    pub(super) fn admit_owner_query(
        query: &AudioOwnerQuery<'plan>,
        control: WorkControl<'_>,
    ) -> Result<(), StageAudioError> {
        control.spend_plan_work(query.work())?;
        Self::validate_owner_query_layers(query)
    }

    /// Shared validation runs for both legacy unpaid and admitted prepaid Q.
    pub(super) fn validate_owner_query_layers(
        query: &AudioOwnerQuery<'plan>,
    ) -> Result<(), StageAudioError> {
        for span in query.spans() {
            deadpan_core::validate_audio_treatment_layers(
                span.owners().iter().filter_map(|owner| owner.treatments()),
            )?;
        }
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn owner_context(
        &self,
        plan: &'plan RenderPlan,
        owner: &AudioOwnerClock<'plan>,
        at: AudioSample,
        control: WorkControl<'_>,
    ) -> Result<Self, StageAudioError> {
        let input = match self {
            Self::Root => plan.audio_owner_input(owner, at, control.query_limits()?)?,
            Self::Domain(domain) => domain.owner_input(owner, at, control.query_limits()?)?,
        };
        control.spend_plan_work(input.work())?;
        Self::finish_paid_owner_input(&input, owner, at, control)
    }

    /// Input construction has already been billed, either in full by
    /// owner_context or as query plus capture by the candidate consumer.
    #[cfg(test)]
    pub(super) fn finish_paid_owner_input(
        input: &deadpan_plan::AudioOwnerInput<'plan>,
        _owner: &AudioOwnerClock<'plan>,
        _at: AudioSample,
        _control: WorkControl<'_>,
    ) -> Result<Self, StageAudioError> {
        Ok(Self::Domain(Arc::new(input.context().clone())))
    }

    pub(super) fn finish_paid_owner_input_admitted<A>(
        input: &deadpan_plan::AudioOwnerInput<'plan>,
        _owner: &AudioOwnerClock<'plan>,
        _at: AudioSample,
        control: WorkControl<'_>,
        admission: &mut A,
    ) -> Result<deadpan_plan::AdmittedOriginalContext<'plan, A::Issuer, A::Lease>, StageAudioError>
    where
        A: deadpan_plan::AudioSoundContextAdmission<Error = StageAudioError>,
    {
        let limits = if A::METERED {
            control.query_limits()?
        } else {
            deadpan_plan::AudioQueryLimits::default()
        };
        input.context().clone_shared_admitted(limits, admission)
    }
}

struct WideBlock {
    samples: Vec<[f64; 2]>,
    suppressed: AudioSuppressionMask,
    dependencies: Dependencies,
    relative_depth: usize,
}

struct Node<'plan, 'work> {
    parent: usize,
    context: Context<'plan>,
    boundary: deadpan_plan::AdmittedAudioTreatmentBoundary<
        'plan,
        sound_context_admission::Issuer<'work>,
        sound_context_admission::Lease<'work>,
    >,
    filter: Option<crate::Equalizer>,
    tiles: tile_index::TileIndex,
}

struct Graph<'plan, 'skip, 'work> {
    plan: &'plan RenderPlan,
    nodes: Vec<Node<'plan, 'work>>,
    resident: u64,
    node_capacity_frames: u64,
    skip: &'skip [authored_gain::OwnerIdentity],
}

impl Graph<'_, '_, '_> {
    fn skips(&self, owner: &AudioOwnerClock<'_>) -> bool {
        !self.skip.is_empty() && self.skip.contains(&authored_gain::identity(owner))
    }
}

// Borrows the local admitted copy, so recursive Graph growth cannot invalidate
// the stop. These are the two predicates the old owned boundary exposed here.
#[derive(Clone, Copy)]
struct StopView<'a, 'plan> {
    owner: &'a AudioOwnerClock<'plan>,
    before: usize,
}

impl StopView<'_, '_> {
    fn matches(self, owner: &AudioOwnerClock<'_>) -> bool {
        self.owner.matches_treatment_owner(owner)
    }

    fn before_stage(self) -> usize {
        self.before
    }
}

#[derive(Clone, Copy)]
struct Program<'a, 'plan> {
    context: &'a Context<'plan>,
    // None is the complete context; Some stops before this owner's EQ.
    stop: Option<StopView<'a, 'plan>>,
    parent: usize,
    depth: usize,
}

impl StageAudio {
    pub(super) fn read_authored_equalizers(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        start: AudioSample,
        frames: u32,
        control: WorkControl<'_>,
        depth: usize,
        skip: &[authored_gain::OwnerIdentity],
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
            node_capacity_frames: 0,
            skip,
        };
        let reservation = u64::from(frames) * 3;
        self.make_room(reservation, false, control)?;
        self.active_frames += reservation;
        let result = (|| {
            let program = Program {
                context: &Context::Root,
                stop: None,
                parent: usize::MAX,
                depth,
            };
            let mut samples = vec![[0.0; 2]; frames as usize];
            let mut suppressed = AudioSuppressionMask::default();
            let mut dependencies = Dependencies::new();
            let mut relative_depth = 0;
            let mut cursor = start;
            while cursor.0 < end {
                let count = u32::try_from((end - cursor.0).min(i64::from(MAX_OUTPUT_FRAMES)))
                    .map_err(|_| StageAudioError::Range)?;
                let block =
                    self.read_eq_program(provider, &mut graph, program, cursor, count, control)?;
                let offset =
                    usize::try_from(cursor.0 - start.0).map_err(|_| StageAudioError::Range)?;
                authored_gain::write_finite_samples(
                    &mut samples[offset..offset + count as usize],
                    block.samples,
                )?;
                suppressed.extend(block.suppressed, &mut self.masks(control))?;
                dependencies.extend(block.dependencies);
                relative_depth = relative_depth.max(block.relative_depth);
                cursor.0 += i64::from(count);
            }
            Ok(ReadBlock {
                start,
                samples,
                suppressed: suppressed.merge(&self.masks(control))?,
                exhausted: Vec::new(),
                dependencies,
                relative_depth,
            })
        })();
        let graph_resident = graph.resident;
        drop(graph);
        self.active_frames -= reservation + graph_resident;
        result
    }

    fn read_eq_program<'plan, 'work>(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        graph: &mut Graph<'plan, '_, 'work>,
        program: Program<'_, 'plan>,
        start: AudioSample,
        frames: u32,
        control: WorkControl<'work>,
    ) -> Result<WideBlock, StageAudioError> {
        self.read_eq_program_with_index_work::<true>(
            provider, graph, program, start, frames, control,
        )
    }

    fn read_eq_program_with_index_work<'plan, 'work, const METER_WORK: bool>(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        graph: &mut Graph<'plan, '_, 'work>,
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
        let range = program.context.support(graph.plan, start..end)?;
        let reservation = u64::from(frames) * 3;
        self.make_room(reservation, false, control)?;
        self.active_frames += reservation;
        let result = (|| {
            let mut result = WideBlock {
                samples: vec![[0.0; 2]; frames as usize],
                suppressed: AudioSuppressionMask::default(),
                dependencies: Dependencies::new(),
                relative_depth: 0,
            };
            if range.start >= range.end {
                result
                    .suppressed
                    .push(start..end, &mut self.masks(control))?;
                return Ok(result);
            }
            if start < range.start {
                result
                    .suppressed
                    .push(start..range.start, &mut self.masks(control))?;
            }
            if range.end < end {
                result
                    .suppressed
                    .push(range.end..end, &mut self.masks(control))?;
            }
            let owners = {
                let mut admission = sound_context_admission::Admission::new(self, control);
                program.context.span_start_owners_admitted(
                    graph.plan,
                    range.clone(),
                    control,
                    &mut admission,
                )?
            };
            let count =
                u32::try_from(range.end.0 - range.start.0).map_err(|_| StageAudioError::Range)?;
            let mut raw = match program.context {
                Context::Root => self.read_controlled(
                    provider,
                    range.start,
                    count,
                    control,
                    true,
                    program.depth,
                )?,
                Context::Domain(domain) => self.read_context_controlled(
                    provider,
                    domain,
                    range.start,
                    count,
                    control,
                    (program.depth, true),
                )?,
            };
            result.dependencies.extend(raw.dependencies);
            result.relative_depth = raw.relative_depth;
            raw.suppressed
                .append_exhausted(&mut raw.exhausted, &mut self.masks(control))?;
            result
                .suppressed
                .extend(raw.suppressed, &mut self.masks(control))?;
            for (span_index, span) in owners.query().spans().iter().enumerate() {
                control.check()?;
                let first = match program.stop {
                    Some(stop) => {
                        match span.owners().iter().position(|owner| stop.matches(owner)) {
                            Some(index) => index,
                            None => {
                                result
                                    .suppressed
                                    .push(span.samples(), &mut self.masks(control))?;
                                continue;
                            }
                        }
                    }
                    None => 0,
                };
                if span.support() == AudioOwnerSupport::Inactive {
                    result
                        .suppressed
                        .push(span.samples(), &mut self.masks(control))?;
                    continue;
                }
                let selected =
                    span.owners()
                        .iter()
                        .enumerate()
                        .skip(first)
                        .find_map(|(index, owner)| {
                            if graph.skips(owner) {
                                return None;
                            }
                            let recipe = owner.treatments()?;
                            if recipe.equalizer()?.is_unity() {
                                return None;
                            }
                            let stage = recipe
                                .order()
                                .iter()
                                .position(|stage| *stage == AudioTreatmentStage::Equalizer)?;
                            if index == first
                                && program
                                    .stop
                                    .is_some_and(|stop| stage >= stop.before_stage())
                            {
                                return None;
                            }
                            Some((index, stage))
                        });
                let node = if let Some((index, stage)) = selected {
                    Some(self.eq_node_selected(
                        graph,
                        program,
                        node_description::Selection {
                            request: node_description::Request {
                                incoming: program.context,
                                owner: &span.owners()[index],
                                at: span.samples().start,
                                stage,
                            },
                            source: owners.source(span_index),
                        },
                        control,
                    )?)
                } else {
                    None
                };
                let mut cached: Option<(i64, Arc<WideBlock>)> = None;
                for sample in span.samples().start.0..span.samples().end.0 {
                    let at = AudioSample(sample);
                    let mut gain = authored_gain::GainSum::default();
                    let value = if let Some(id) = node {
                        let tile = sample
                            .div_euclid(OUTPUT as i64)
                            .checked_mul(OUTPUT as i64)
                            .ok_or(TimeError::Overflow)?;
                        if cached.as_ref().is_none_or(|(key, _)| *key != tile) {
                            let block = self.eq_tile_with_index_work::<METER_WORK>(
                                provider,
                                graph,
                                id,
                                tile,
                                control,
                                program.depth + 1,
                            )?;
                            result.dependencies.extend(
                                block
                                    .dependencies
                                    .iter()
                                    .map(|(asset, fingerprint)| (asset.clone(), *fingerprint)),
                            );
                            result.relative_depth = result.relative_depth.max(block.relative_depth);
                            result.suppressed.extend_borrowed_filter_map(
                                &block.suppressed,
                                |r| {
                                    let r = r.start.max(span.samples().start)
                                        ..r.end.min(span.samples().end);
                                    (r.start < r.end).then_some(r)
                                },
                                &mut self.masks(control),
                            )?;
                            cached = Some((tile, block));
                        }
                        let (_, block) = cached
                            .as_ref()
                            .ok_or(PlanError::InvalidPlan("EQ tile absent"))?;
                        block.samples[(sample - tile) as usize]
                    } else {
                        raw.samples[(sample - range.start.0) as usize].map(f64::from)
                    };
                    let last = selected.map_or(span.owners().len(), |(index, _)| index + 1);
                    for index in (first..last).rev() {
                        let owner = &span.owners()[index];
                        if graph.skips(owner) {
                            continue;
                        }
                        let after = selected
                            .filter(|(chosen, _)| *chosen == index)
                            .map_or(0, |(_, stage)| stage + 1);
                        let until = if index == first {
                            program.stop.map_or_else(
                                || owner.treatments().map_or(0, |recipe| recipe.order().len()),
                                StopView::before_stage,
                            )
                        } else {
                            owner.treatments().map_or(0, |recipe| recipe.order().len())
                        };
                        gain.owner_slice(owner, at, after..until, after == 0, control)?;
                    }
                    result.samples[(sample - start.0) as usize] = gain.apply_wide(value)?;
                }
            }
            result.suppressed = result.suppressed.merge(&self.masks(control))?;
            for range in &result.suppressed {
                for at in range.start.max(start).0..range.end.min(end).0 {
                    result.samples[(at - start.0) as usize] = [0.0; 2];
                }
            }
            control.check()?;
            Ok(result)
        })();
        self.active_frames -= reservation;
        result
    }

    #[cfg(test)]
    fn eq_node<'plan, 'work>(
        &mut self,
        graph: &mut Graph<'plan, '_, 'work>,
        program: Program<'_, 'plan>,
        owner: &AudioOwnerClock<'plan>,
        at: AudioSample,
        stage: usize,
        control: WorkControl<'work>,
    ) -> Result<usize, StageAudioError> {
        self.eq_node_selected(
            graph,
            program,
            node_description::Selection {
                request: node_description::Request {
                    incoming: program.context,
                    owner,
                    at,
                    stage,
                },
                source: node_description::Source::Direct,
            },
            control,
        )
    }

    fn eq_node_selected<'plan, 'work>(
        &mut self,
        graph: &mut Graph<'plan, '_, 'work>,
        program: Program<'_, 'plan>,
        selection: node_description::Selection<
            '_,
            '_,
            'plan,
            sound_context_admission::Issuer<'work>,
            sound_context_admission::Lease<'work>,
        >,
        control: WorkControl<'work>,
    ) -> Result<usize, StageAudioError> {
        let node_description::Request {
            owner, at, stage, ..
        } = selection.request;
        if let Some(index) = graph.nodes.iter().position(|node| {
            node.parent == program.parent
                && node.boundary.before_stage() == stage
                && node.boundary.matches_clock_at(owner, at)
        }) {
            return Ok(index);
        }
        if graph.nodes.len() >= self.limits.maximum_prepared_stages as usize {
            return Err(StageAudioError::Limit("EQ owner count"));
        }
        let mut description = {
            let mut admission = sound_context_admission::Admission::new(self, control);
            node_description::describe_selected_admitted(
                graph.plan,
                selection,
                control,
                &mut admission,
            )?
        };
        let original_frames = description
            .original
            .as_ref()
            .map(|value| {
                value
                    .leases()
                    .into_iter()
                    .flatten()
                    .try_fold(0_u64, |sum, lease| {
                        sum.checked_add(lease.frames())
                            .ok_or(StageAudioError::Range)
                    })
            })
            .transpose()?
            .unwrap_or(0);
        let boundary_frames = description
            .boundary
            .lease()
            .map_or(0, sound_context_admission::Lease::frames);
        let metadata = original_frames
            .checked_add(boundary_frames)
            .ok_or(StageAudioError::Range)?;
        let expected_issuer = sound_context_admission::issuer(control);
        if description.boundary.issuer() != expected_issuer
            || description
                .original
                .as_ref()
                .is_some_and(|value| value.issuer() != expected_issuer)
        {
            return Err(PlanError::AudioMetadataIssuerMismatch.into());
        }
        let resident = u64::try_from(crate::Equalizer::preparation_bytes_bound())
            .map_err(|_| StageAudioError::Range)?
            .div_ceil(std::mem::size_of::<[f32; 2]>() as u64);
        // Keep compilation in its old position, after metadata resolution.
        control.spend_plan_work(deadpan_dsp::FINITE_FILTER_TAPS)?;
        let maximum_nodes = self.limits.maximum_prepared_stages as usize;
        let mut capacity = shared_eq::node_capacity::prepare(
            self,
            &graph.nodes,
            graph.node_capacity_frames,
            maximum_nodes,
            control,
        )?;
        let retained = resident
            .checked_add(metadata)
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
        self.make_room(resident, false, control)?;
        let filter = crate::Equalizer::new(description.recipe, control.cancelled)?;
        control.check()?;
        capacity.allocate()?;
        capacity.prepay_moves(control)?;
        capacity.validate_current(&graph.nodes, graph.node_capacity_frames)?;
        control.check()?;
        let node_description::Description { values, original } = &mut description;
        let context_transfer = {
            let mut admission = sound_context_admission::Admission::new(self, control);
            deadpan_plan::preflight_context_transfer(original.as_mut(), None, &mut admission)?
        };
        let [plan, core, buffer, sound] = context_transfer.leases();
        let transfer = sound_context_admission::preflight_transfer(
            control,
            sound_context_admission::TransferLeases {
                original: [plan, core, buffer],
                sound,
                key: None,
                boundary: values
                    .as_ref()
                    .expect("unpublished Authored description")
                    .boundary
                    .lease(),
                capacity: capacity.lease(),
            },
        )?;
        let id = graph.nodes.len();
        context_transfer.commit(|plan, core, buffer, sound| {
            let node_description::DescriptionValues {
                context,
                boundary,
                recipe: _,
            } = values.take().expect("unpublished Authored description");
            capacity.publish(
                &mut graph.nodes,
                Node {
                    parent: program.parent,
                    context,
                    boundary,
                    filter: Some(filter),
                    tiles: tile_index::TileIndex::new(),
                },
            );
            // The old allocation has physically dropped before its charge retires.
            self.active_frames = next_active;
            graph.resident = next_resident;
            graph.node_capacity_frames = next_capacity_frames;
            graph.nodes[id].boundary.with_lease_mut(|boundary| {
                transfer.commit(sound_context_admission::TransferLeasesMut {
                    original: [plan, core, buffer],
                    sound,
                    key: None,
                    boundary,
                    capacity: capacity.lease_mut(),
                });
            });
        });
        Ok(id)
    }

    #[cfg(test)]
    fn eq_tile<'plan, 'work>(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        graph: &mut Graph<'plan, '_, 'work>,
        id: usize,
        tile: i64,
        control: WorkControl<'work>,
        depth: usize,
    ) -> Result<Arc<WideBlock>, StageAudioError> {
        self.eq_tile_with_index_work::<true>(provider, graph, id, tile, control, depth)
    }

    fn eq_tile_with_index_work<'plan, 'work, const METER_WORK: bool>(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        graph: &mut Graph<'plan, '_, 'work>,
        id: usize,
        tile: i64,
        control: WorkControl<'work>,
        depth: usize,
    ) -> Result<Arc<WideBlock>, StageAudioError> {
        control.check()?;
        if depth > self.limits.maximum_depth {
            return Err(StageAudioError::Limit("nested stage depth"));
        }
        if let Some(block) = graph.nodes[id].tiles.get::<METER_WORK>(tile, control)? {
            if depth
                .checked_sub(1)
                .and_then(|parent| parent.checked_add(block.relative_depth))
                .is_none_or(|value| value > self.limits.maximum_depth)
            {
                return Err(StageAudioError::Limit("nested stage depth"));
            }
            return Ok(Arc::clone(block));
        }
        let begin = tile.checked_sub(RADIUS as i64).ok_or(TimeError::Overflow)?;
        let end = begin.checked_add(INPUT as i64).ok_or(TimeError::Overflow)?;
        let context = graph.nodes[id].context.clone();
        let boundary = {
            let mut admission = sound_context_admission::Admission::new(self, control);
            let boundary = &graph.nodes[id].boundary;
            boundary.owner().treatment_boundary_admitted(
                boundary.before_stage(),
                control.query_limits()?,
                &mut admission,
            )?
        };
        let stop = StopView {
            owner: boundary.owner(),
            before: boundary.before_stage(),
        };
        let mut filter = graph.nodes[id]
            .filter
            .take()
            .ok_or(PlanError::InvalidPlan("recursive EQ cycle"))?;
        let reservation = ((INPUT + OUTPUT * 2) * 2) as u64;
        // Admission can fail after taking the worker. Always restore it below.
        let result = (|| {
            self.reserve_eq_tile(reservation, control)?;
            let result = (|| {
                let mut left = vec![0.0; INPUT];
                let mut right = vec![0.0; INPUT];
                let mut suppressed = AudioSuppressionMask::default();
                let mut dependencies = Dependencies::new();
                let mut relative_depth = 1;
                let mut at = begin;
                while at < end {
                    let count = u32::try_from((end - at).min(i64::from(MAX_OUTPUT_FRAMES)))
                        .map_err(|_| StageAudioError::Range)?;
                    let block = self.read_eq_program_with_index_work::<METER_WORK>(
                        provider,
                        graph,
                        Program {
                            context: &context,
                            stop: Some(stop),
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
                    suppressed.extend_filter_map(
                        block.suppressed,
                        |r| {
                            let r = r.start.max(AudioSample(tile))
                                ..r.end.min(AudioSample(tile + OUTPUT as i64));
                            (r.start < r.end).then_some(r)
                        },
                        &mut self.masks(control),
                    )?;
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
                let mut samples: Vec<_> = out_left
                    .into_iter()
                    .zip(out_right)
                    .map(|(l, r)| [l, r])
                    .collect();
                let suppressed = suppressed.merge(&self.masks(control))?;
                for r in &suppressed {
                    for at in r.start.0..r.end.0 {
                        samples[(at - tile) as usize] = [0.0; 2];
                    }
                }
                control.check()?;
                let resident = (OUTPUT * 2) as u64;
                // Prepare against the actual index after recursion. Both old
                // storage and the complete replacement stay live at this peak.
                let mut index =
                    tile_index::prepare::<METER_WORK>(self, &graph.nodes[id].tiles, tile, control)?;
                let mut block = tile_block::PendingArc::new(
                    self,
                    WideBlock {
                        samples,
                        suppressed,
                        dependencies,
                        relative_depth,
                    },
                    control,
                )?;
                let retained = resident
                    .checked_add(index.new_frames())
                    .and_then(|value| value.checked_add(block.frames()))
                    .ok_or(StageAudioError::Range)?;
                let next_active = self
                    .active_frames
                    .checked_sub(index.retired_frames())
                    .and_then(|value| value.checked_add(retained))
                    .ok_or(StageAudioError::Range)?;
                let next_resident = graph
                    .resident
                    .checked_sub(index.retired_frames())
                    .and_then(|value| value.checked_add(retained))
                    .ok_or(StageAudioError::Range)?;
                // The existing retained PCM bound sees both new metadata
                // leases; samples themselves are not billed a second time.
                self.make_room(resident, false, control)?;
                index.allocate()?;
                index.prepay_moves::<METER_WORK>(control)?;
                index.validate_current::<METER_WORK>(&graph.nodes[id].tiles, control)?;
                control.check()?;
                let transfer = sound_context_admission::preflight_tile_transfer(
                    control,
                    sound_context_admission::TileTransferLeases {
                        index: index.lease(),
                        arc_shell: block.lease(),
                    },
                )?;
                index.publish(
                    &mut graph.nodes[id].tiles,
                    Arc::clone(block.value.as_ref().expect("unpublished tile")),
                );
                // publish physically dropped the old Vec before retirement.
                self.active_frames = next_active;
                graph.resident = next_resident;
                transfer.commit(sound_context_admission::TileTransferLeasesMut {
                    index: index.lease_mut(),
                    arc_shell: block.lease_mut(),
                });
                Ok(block.value.take().expect("published tile"))
            })();
            self.active_frames -= reservation;
            result
        })();
        graph.nodes[id].filter = Some(filter);
        result
    }

    pub(super) fn reserve_eq_tile(
        &mut self,
        resident: u64,
        control: WorkControl<'_>,
    ) -> Result<(), StageAudioError> {
        if INPUT as u32 > self.limits.maximum_input_frames {
            return Err(StageAudioError::Limit("input frames"));
        }
        if OUTPUT as u32 > self.limits.maximum_output_frames {
            return Err(StageAudioError::Limit("output frames"));
        }
        // A finite tile is a window of one EQ owner, not a fresh canonical
        // stretch stage. Owner count and coefficient work are admitted above;
        // all tile input/output still spends the cumulative frame allowance.
        {
            let mut work = control.work.borrow_mut();
            work.prepared_frames += (INPUT + OUTPUT) as u64;
            if work.prepared_frames > self.limits.maximum_prepared_frames {
                return Err(StageAudioError::Limit("prepared frames per read"));
            }
        }
        self.make_room(resident, false, control)?;
        self.active_frames += resident;
        Ok(())
    }
}
