//! Recursive ordered EQ over Original owners. One request owns all filtered
//! tiles, source dependencies and resource admission; immutable contexts cannot
//! be reconstructed from node IDs or nominal automation coordinates alone.

#[cfg(test)]
#[path = "authored_eq/wide_query_tests.rs"]
mod wide_query_tests;

use deadpan_core::AudioTreatmentStage;
use deadpan_dsp::{
    FINITE_FILTER_INPUT_FRAMES as INPUT, FINITE_FILTER_OUTPUT_FRAMES as OUTPUT,
    FINITE_FILTER_RADIUS as RADIUS,
};
use deadpan_plan::{
    AudioOwnerClock, AudioOwnerQuery, AudioOwnerRetainedInput, AudioOwnerSupport,
    AudioTreatmentBoundary, SharedAudioOwnerRetainedInput,
};

use super::*;

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
    Retained(SharedAudioOwnerRetainedInput<'plan>),
}

trait FinalOriginalSharing<'plan> {
    type Output;
    type Probe;
    type Owners;

    fn probe(
        &mut self,
        input: &deadpan_plan::AudioOwnerInput<'plan>,
        at: AudioSample,
        limits: deadpan_plan::AudioQueryLimits,
    ) -> Result<Self::Probe, StageAudioError>;
    fn probe_input(probe: &Self::Probe) -> Option<&AudioOwnerRetainedInput<'plan>>;
    fn probe_work(probe: &Self::Probe) -> usize;
    fn probe_was_forwarded(probe: &Self::Probe) -> bool;
    fn owners(
        &mut self,
        retained: &AudioOwnerRetainedInput<'plan>,
        samples: Range<AudioSample>,
        limits: deadpan_plan::AudioQueryLimits,
    ) -> Result<Self::Owners, StageAudioError>;
    fn owners_query(owners: &Self::Owners) -> &AudioOwnerQuery<'plan>;
    fn owners_work(owners: &Self::Owners) -> usize;
    fn owners_was_forwarded(owners: &Self::Owners) -> bool;
    fn retained(self, probe: Self::Probe) -> Result<Self::Output, StageAudioError>;
    fn domain(self, value: &AudioDomain<'plan>) -> Result<Self::Output, StageAudioError>;
}

struct LegacyOriginalSharing;
impl<'plan> FinalOriginalSharing<'plan> for LegacyOriginalSharing {
    type Output = Context<'plan>;
    type Probe = deadpan_plan::AudioOwnerRetainedProbe<'plan>;
    type Owners = AudioOwnerQuery<'plan>;

    fn probe(
        &mut self,
        input: &deadpan_plan::AudioOwnerInput<'plan>,
        at: AudioSample,
        limits: deadpan_plan::AudioQueryLimits,
    ) -> Result<Self::Probe, StageAudioError> {
        Ok(input.retained_probe_at(at, limits)?)
    }
    fn probe_input(probe: &Self::Probe) -> Option<&AudioOwnerRetainedInput<'plan>> {
        probe.input()
    }
    fn probe_work(probe: &Self::Probe) -> usize {
        probe.work()
    }
    fn probe_was_forwarded(_: &Self::Probe) -> bool {
        false
    }
    fn owners(
        &mut self,
        retained: &AudioOwnerRetainedInput<'plan>,
        samples: Range<AudioSample>,
        limits: deadpan_plan::AudioQueryLimits,
    ) -> Result<Self::Owners, StageAudioError> {
        Ok(retained.owners(samples, limits)?)
    }
    fn owners_query(owners: &Self::Owners) -> &AudioOwnerQuery<'plan> {
        owners
    }
    fn owners_work(owners: &Self::Owners) -> usize {
        owners.work()
    }
    fn owners_was_forwarded(_: &Self::Owners) -> bool {
        false
    }
    fn retained(self, probe: Self::Probe) -> Result<Self::Output, StageAudioError> {
        Ok(Context::Retained(
            probe
                .into_input()
                .expect("accepted retained probe")
                .into_shared(),
        ))
    }
    fn domain(self, value: &AudioDomain<'plan>) -> Result<Self::Output, StageAudioError> {
        Ok(Context::Domain(Arc::new(value.clone())))
    }
}

struct AdmittedOriginalSharing<'admission, 'work, A> {
    control: WorkControl<'work>,
    admission: &'admission mut A,
}
impl<A> AdmittedOriginalSharing<'_, '_, A>
where
    A: deadpan_plan::AudioSoundContextAdmission<Error = StageAudioError>,
{
    fn limits(&self) -> Result<deadpan_plan::AudioQueryLimits, StageAudioError> {
        if A::METERED {
            self.control.query_limits()
        } else {
            Ok(deadpan_plan::AudioQueryLimits::default())
        }
    }
}
impl<'plan, A> FinalOriginalSharing<'plan> for AdmittedOriginalSharing<'_, '_, A>
where
    A: deadpan_plan::AudioSoundContextAdmission<Error = StageAudioError>,
{
    type Output = deadpan_plan::AdmittedOriginalContext<'plan, A::Issuer, A::Lease>;
    type Probe = deadpan_plan::AdmittedAudioOwnerRetainedProbe<'plan, A::Issuer, A::Lease>;
    type Owners = deadpan_plan::AdmittedAudioOwnerQuery<'plan, A::Issuer, A::Lease>;

    fn probe(
        &mut self,
        input: &deadpan_plan::AudioOwnerInput<'plan>,
        at: AudioSample,
        limits: deadpan_plan::AudioQueryLimits,
    ) -> Result<Self::Probe, StageAudioError> {
        let probe = input.retained_probe_at_admitted(at, limits, self.admission)?;
        if probe.was_forwarded() != A::METERED {
            return Err(PlanError::InvalidPlan("retained probe accounting mode differs").into());
        }
        Ok(probe)
    }
    fn probe_input(probe: &Self::Probe) -> Option<&AudioOwnerRetainedInput<'plan>> {
        probe.input()
    }
    fn probe_work(probe: &Self::Probe) -> usize {
        probe.work()
    }
    fn probe_was_forwarded(probe: &Self::Probe) -> bool {
        probe.was_forwarded()
    }
    fn owners(
        &mut self,
        retained: &AudioOwnerRetainedInput<'plan>,
        samples: Range<AudioSample>,
        limits: deadpan_plan::AudioQueryLimits,
    ) -> Result<Self::Owners, StageAudioError> {
        let owners = retained.owners_admitted(samples, limits, self.admission)?;
        if owners.was_forwarded() != A::METERED {
            return Err(PlanError::InvalidPlan("retained owners accounting mode differs").into());
        }
        Ok(owners)
    }
    fn owners_query(owners: &Self::Owners) -> &AudioOwnerQuery<'plan> {
        owners.query()
    }
    fn owners_work(owners: &Self::Owners) -> usize {
        owners.work()
    }
    fn owners_was_forwarded(owners: &Self::Owners) -> bool {
        owners.was_forwarded()
    }
    fn retained(self, probe: Self::Probe) -> Result<Self::Output, StageAudioError> {
        probe.into_shared_admitted(self.limits()?, self.admission)
    }
    fn domain(self, value: &AudioDomain<'plan>) -> Result<Self::Output, StageAudioError> {
        value.clone_shared_admitted(self.limits()?, self.admission)
    }
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
            Self::Retained(_) => return Ok(range),
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
            Self::Retained(input) => input.owners_admitted(range, limits, admission)?,
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
            Self::Retained(input) => input.owners(range, control.query_limits()?)?,
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
            // An enclosing retained processor must stay on its exact consumer
            // grid. Owner membership is checked by this context's query.
            Self::Retained(input) => return Ok(Self::Retained(input.clone())),
        };
        control.spend_plan_work(input.work())?;
        Self::finish_paid_owner_input(&input, owner, at, control)
    }

    /// Input construction has already been billed, either in full by
    /// owner_context or as query plus capture by the candidate consumer.
    pub(super) fn finish_paid_owner_input(
        input: &deadpan_plan::AudioOwnerInput<'plan>,
        owner: &AudioOwnerClock<'plan>,
        at: AudioSample,
        control: WorkControl<'_>,
    ) -> Result<Self, StageAudioError> {
        Self::finish_paid_owner_input_with(input, owner, at, control, LegacyOriginalSharing)
    }

    pub(super) fn finish_paid_owner_input_admitted<A>(
        input: &deadpan_plan::AudioOwnerInput<'plan>,
        owner: &AudioOwnerClock<'plan>,
        at: AudioSample,
        control: WorkControl<'_>,
        admission: &mut A,
    ) -> Result<deadpan_plan::AdmittedOriginalContext<'plan, A::Issuer, A::Lease>, StageAudioError>
    where
        A: deadpan_plan::AudioSoundContextAdmission<Error = StageAudioError>,
    {
        Self::finish_paid_owner_input_with(
            input,
            owner,
            at,
            control,
            AdmittedOriginalSharing { control, admission },
        )
    }

    // Both strategies use the same actual probe, owners and boundary decision.
    // The admitted strategy keeps its real probe guarded through final sharing.
    fn finish_paid_owner_input_with<S: FinalOriginalSharing<'plan>>(
        input: &deadpan_plan::AudioOwnerInput<'plan>,
        owner: &AudioOwnerClock<'plan>,
        at: AudioSample,
        control: WorkControl<'_>,
        mut share: S,
    ) -> Result<S::Output, StageAudioError> {
        let probe = share.probe(input, at, control.query_limits()?)?;
        if !S::probe_was_forwarded(&probe) {
            control.spend_plan_work(S::probe_work(&probe))?;
        }
        if let Some(retained) = S::probe_input(&probe) {
            let end = AudioSample(at.0.checked_add(1).ok_or(TimeError::Overflow)?);
            let owners = share.owners(retained, at..end, control.query_limits()?)?;
            if !S::owners_was_forwarded(&owners) {
                control.spend_plan_work(S::owners_work(&owners))?;
            }
            if S::owners_query(&owners).spans().iter().any(|span| {
                span.owners()
                    .iter()
                    .any(|candidate| owner.matches_treatment_owner(candidate))
            }) {
                return share.retained(probe);
            }
        }
        // Rejected Some and positive-work None release their actual probe
        // before the existing fallback Domain copy begins.
        drop(probe);
        share.domain(input.context())
    }
}

struct WideBlock {
    samples: Vec<[f64; 2]>,
    suppressed: Vec<Range<AudioSample>>,
    dependencies: Dependencies,
    relative_depth: usize,
}

struct Node<'plan> {
    parent: usize,
    context: Context<'plan>,
    boundary: AudioTreatmentBoundary<'plan>,
    filter: Option<crate::Equalizer>,
    tiles: BTreeMap<i64, Arc<WideBlock>>,
}

struct Graph<'plan, 'skip> {
    plan: &'plan RenderPlan,
    nodes: Vec<Node<'plan>>,
    resident: u64,
    skip: &'skip [authored_gain::OwnerIdentity],
}

impl Graph<'_, '_> {
    fn skips(&self, owner: &AudioOwnerClock<'_>) -> bool {
        !self.skip.is_empty() && self.skip.contains(&authored_gain::identity(owner))
    }
}

#[derive(Clone, Copy)]
struct Program<'a, 'plan> {
    context: &'a Context<'plan>,
    // None is the complete context; Some stops before this owner's EQ.
    stop: Option<&'a AudioTreatmentBoundary<'plan>>,
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
            let mut suppressed = Vec::new();
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
                suppressed.extend(block.suppressed);
                dependencies.extend(block.dependencies);
                relative_depth = relative_depth.max(block.relative_depth);
                cursor.0 += i64::from(count);
            }
            Ok(ReadBlock {
                start,
                samples,
                suppressed: merged_suppression(suppressed),
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

    fn read_eq_program<'plan>(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        graph: &mut Graph<'plan, '_>,
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
        let range = program.context.support(graph.plan, start..end)?;
        let reservation = u64::from(frames) * 3;
        self.make_room(reservation, false, control)?;
        self.active_frames += reservation;
        let result = (|| {
            let mut result = WideBlock {
                samples: vec![[0.0; 2]; frames as usize],
                suppressed: Vec::new(),
                dependencies: Dependencies::new(),
                relative_depth: 0,
            };
            if range.start >= range.end {
                result.suppressed.push(start..end);
                return Ok(result);
            }
            if start < range.start {
                result.suppressed.push(start..range.start);
            }
            if range.end < end {
                result.suppressed.push(range.end..end);
            }
            let owners = {
                let mut admission = sound_context_admission::Admission::new(self, control);
                program.context.owners_admitted(
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
                Context::Retained(input) => self.read_retained_controlled(
                    provider,
                    input,
                    range.start,
                    count,
                    control,
                    (program.depth, false),
                )?,
            };
            result.dependencies.extend(raw.dependencies);
            result.relative_depth = raw.relative_depth;
            raw.suppressed.append(&mut raw.exhausted);
            result.suppressed.extend(raw.suppressed);
            for span in owners.query().spans() {
                control.check()?;
                let first = match program.stop {
                    Some(stop) => {
                        match span.owners().iter().position(|owner| stop.matches(owner)) {
                            Some(index) => index,
                            None => {
                                result.suppressed.push(span.samples());
                                continue;
                            }
                        }
                    }
                    None => 0,
                };
                if span.support() == AudioOwnerSupport::Inactive {
                    result.suppressed.push(span.samples());
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
                    Some(self.eq_node(
                        graph,
                        program,
                        &span.owners()[index],
                        span.samples().start,
                        stage,
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
                            let block = self.eq_tile(
                                provider,
                                graph,
                                id,
                                tile,
                                control,
                                program.depth + 1,
                            )?;
                            result.dependencies.extend(block.dependencies.clone());
                            result.relative_depth = result.relative_depth.max(block.relative_depth);
                            result
                                .suppressed
                                .extend(block.suppressed.iter().filter_map(|r| {
                                    let r = r.start.max(span.samples().start)
                                        ..r.end.min(span.samples().end);
                                    (r.start < r.end).then_some(r)
                                }));
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
                                AudioTreatmentBoundary::before_stage,
                            )
                        } else {
                            owner.treatments().map_or(0, |recipe| recipe.order().len())
                        };
                        gain.owner_slice(owner, at, after..until, after == 0, control)?;
                    }
                    result.samples[(sample - start.0) as usize] = gain.apply_wide(value)?;
                }
            }
            result.suppressed = merged_suppression(result.suppressed);
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

    fn eq_node<'plan>(
        &mut self,
        graph: &mut Graph<'plan, '_>,
        program: Program<'_, 'plan>,
        owner: &AudioOwnerClock<'plan>,
        at: AudioSample,
        stage: usize,
        control: WorkControl<'_>,
    ) -> Result<usize, StageAudioError> {
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
        let context = program
            .context
            .owner_context(graph.plan, owner, at, control)?;
        let recipe = owner
            .treatments()
            .and_then(|recipe| recipe.equalizer())
            .ok_or(PlanError::InvalidPlan("EQ recipe absent"))?;
        let boundary = owner.treatment_boundary(stage)?;
        let resident = u64::try_from(crate::Equalizer::preparation_bytes_bound())
            .map_err(|_| StageAudioError::Range)?
            .div_ceil(std::mem::size_of::<[f32; 2]>() as u64);
        let next_active = self
            .active_frames
            .checked_add(resident)
            .ok_or(StageAudioError::Range)?;
        let next_resident = graph
            .resident
            .checked_add(resident)
            .ok_or(StageAudioError::Range)?;
        // Compilation is fixed work, independent of a tile's sample count.
        // Charge it even when another preparation compiled this same owner.
        control.spend_plan_work(deadpan_dsp::FINITE_FILTER_TAPS)?;
        self.make_room(resident, false, control)?;
        let filter = crate::Equalizer::new(recipe, control.cancelled)?;
        control.check()?;
        graph.nodes.push(Node {
            parent: program.parent,
            context,
            boundary,
            filter: Some(filter),
            tiles: BTreeMap::new(),
        });
        // The constructor is synchronous. Admit its complete peak first, then
        // retain that same bound while this graph owns the published filter.
        self.active_frames = next_active;
        graph.resident = next_resident;
        Ok(graph.nodes.len() - 1)
    }

    fn eq_tile(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        graph: &mut Graph<'_, '_>,
        id: usize,
        tile: i64,
        control: WorkControl<'_>,
        depth: usize,
    ) -> Result<Arc<WideBlock>, StageAudioError> {
        control.check()?;
        if depth > self.limits.maximum_depth {
            return Err(StageAudioError::Limit("nested stage depth"));
        }
        if let Some(block) = graph.nodes[id].tiles.get(&tile) {
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
        let boundary = graph.nodes[id].boundary.clone();
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
                let mut suppressed = Vec::new();
                let mut dependencies = Dependencies::new();
                let mut relative_depth = 1;
                let mut at = begin;
                while at < end {
                    let count = u32::try_from((end - at).min(i64::from(MAX_OUTPUT_FRAMES)))
                        .map_err(|_| StageAudioError::Range)?;
                    let block = self.read_eq_program(
                        provider,
                        graph,
                        Program {
                            context: &context,
                            stop: Some(&boundary),
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
                    suppressed.extend(block.suppressed.into_iter().filter_map(|r| {
                        let r = r.start.max(AudioSample(tile))
                            ..r.end.min(AudioSample(tile + OUTPUT as i64));
                        (r.start < r.end).then_some(r)
                    }));
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
                let suppressed = merged_suppression(suppressed);
                for r in &suppressed {
                    for at in r.start.0..r.end.0 {
                        samples[(at - tile) as usize] = [0.0; 2];
                    }
                }
                control.check()?;
                let resident = (OUTPUT * 2) as u64;
                self.make_room(resident, false, control)?;
                self.active_frames += resident;
                graph.resident += resident;
                let block = Arc::new(WideBlock {
                    samples,
                    suppressed,
                    dependencies,
                    relative_depth,
                });
                graph.nodes[id].tiles.insert(tile, Arc::clone(&block));
                Ok(block)
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
