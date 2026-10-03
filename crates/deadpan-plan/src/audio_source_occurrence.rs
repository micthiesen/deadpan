//! One independent catalog voice through its checked current structural occurrence.

use deadpan_core::{MAX_DOCUMENT_DEPTH, SourceAudio};

use super::*;
use crate::{
    AudioHoldPolicyQuery, AudioPolicyQuery, AudioSignalTape, AudioSignalTapeRun,
    AudioStageProjection,
};

/// A borrowed preparation recipe for one concrete current occurrence.
///
/// The constructor resolves structure and stable plays, never a cursor or a
/// caller-supplied placement. Only enclosing Preserve stages process this voice;
/// a voice owned on a stage enters after that stage. Each stage retains its full
/// intrinsic history independently of Original audio and every other voice.
///
/// `processing` is raw input, including sound underneath silent Holds. `policy`
/// and `hold_policy` expose current output gates separately. Gain, creative
/// edges, allowances, tails and authored event persistence are not installed by
/// this handle. The host must admit `source()` even for a wholly silent query.
#[derive(Debug, Clone)]
pub struct AudioSourceOccurrence<'plan> {
    plan: &'plan RenderPlan,
    instance: InstancePath,
    voice: AudioSourceVoice<'plan>,
    extent: Range<ExactRatio>,
    samples: Range<AudioSample>,
    grid: AudioSampleGrid<AudioSample>,
    // This private carrier is evaluated directly on RoundEven before any PCM
    // exists. PointCeil preparation arrays are never relabeled as root output.
    input: AudioSignalTape<'plan>,
    construction_work: usize,
    retained_runs: usize,
}

#[derive(Clone, Copy)]
struct Placement {
    origin: ExactRatio,
    scale: ExactRatio,
}

impl Placement {
    const IDENTITY: Self = Self {
        origin: ExactRatio::ZERO,
        scale: ExactRatio::ONE,
    };

    fn at(self, local: ExactRatio) -> Result<ExactRatio, PlanError> {
        Ok(self.origin.checked_add(local.checked_mul(self.scale)?)?)
    }

    fn local(self, outer: ExactRatio) -> Result<ExactRatio, PlanError> {
        Ok(outer.checked_sub(self.origin)?.checked_div(self.scale)?)
    }

    fn compose(self, inner: Self) -> Result<Self, PlanError> {
        Ok(Self {
            origin: self.at(inner.origin)?,
            scale: self.scale.checked_mul(inner.scale)?,
        })
    }

    fn range(self, local: Range<ExactRatio>) -> Result<Range<ExactRatio>, PlanError> {
        Ok(self.at(local.start)?..self.at(local.end)?)
    }
}

struct Edge {
    parent: usize,
    map: Placement,
    repeats: usize,
}

struct BuildBudget {
    maximum: usize,
    remaining: usize,
    maximum_runs: usize,
    runs: usize,
}

impl BuildBudget {
    fn spend(&mut self, count: usize) -> Result<(), PlanError> {
        self.remaining = self
            .remaining
            .checked_sub(count)
            .ok_or(PlanError::AudioQueryLimit(
                "source occurrence construction work",
            ))?;
        Ok(())
    }

    fn runs(&mut self, count: usize) -> Result<(), PlanError> {
        self.spend(count)?;
        self.runs = self
            .runs
            .checked_add(count)
            .ok_or(PlanError::AudioQueryLimit("source occurrence runs"))?;
        if self.runs > self.maximum_runs {
            return Err(PlanError::AudioQueryLimit("source occurrence runs"));
        }
        Ok(())
    }
}

impl RenderPlan {
    /// Project one natural-rate catalog voice from a current concrete owner's
    /// output clock. No default Repeat definition or synthetic gap is an owner.
    /// Active explicit gap-override nodes have ordinary concrete occurrences.
    ///
    /// The complete selected recipe must fit its host; overflow is rejected.
    /// Ancestor Retime selections may crop its current allocation without
    /// changing source phase or restarting any retained processing history.
    /// Empty hosts and occurrences with no contributing input are rejected.
    /// An outer crop may hide the geometric owner while retaining its processed
    /// output from an inner Preserve stage.
    ///
    /// Depth is bounded by MAX_DOCUMENT_DEPTH. Compact Repeat segments and
    /// Sequence entries are charged before lookup, never expanded into plays.
    /// `maximum_spans` bounds aggregate retained runs across all stages;
    /// `maximum_work` also covers conservative graph-validation work. Existing
    /// tape and projection graph bounds remain in force.
    pub fn source_voice_occurrence(
        &self,
        instance: InstancePath,
        recipe: AudioSourceVoiceRecipe,
        limits: AudioQueryLimits,
    ) -> Result<AudioSourceOccurrence<'_>, PlanError> {
        limits.validate()?;
        if instance.repeats.len() > MAX_DOCUMENT_DEPTH {
            return Err(PlanError::AudioQueryLimit("source occurrence depth"));
        }
        let mut budget = BuildBudget {
            maximum: limits.maximum_work,
            remaining: limits.maximum_work,
            maximum_runs: limits.maximum_spans,
            runs: 0,
        };
        let host =
            *self
                .by_id
                .get(&instance.node)
                .ok_or(PlanError::InvalidAudioSourceOccurrence(
                    "source occurrence owner is absent",
                ))?;
        let (edges, depth) = resolve_edges(self, host, &instance, &mut budget)?;
        let host_support = node_support(self, host);
        if !positive_range(&host_support)? {
            return Err(PlanError::InvalidAudioSourceOccurrence(
                "source occurrence owner is empty",
            ));
        }
        // Visibility follows the preparation chain below. After Preserve, its
        // complete processed output can survive an outer crop even when that
        // crop hides this owner's original geometric allocation.
        let rate = self.metadata.presentation_basis.frame_rate;
        budget.spend(1 + instance.repeats.len())?;
        let owner = scoped_signal(self, host, instance.repeats.clone(), host_support.clone());
        let voice = checked_voice(owner, recipe)?;
        let mut provider = Provider::Source(Box::new(voice.input_signal()));
        let mut support = host_support;
        let mut placement = Placement::IDENTITY;
        let mut stages = 0usize;
        for edge in edges.iter().rev() {
            let parent = &self.nodes[edge.parent];
            if matches!(parent.kind, CompiledKind::Retime { pitch: PitchPolicy::Preserve, scale, .. } if scale != ExactRatio::ONE)
            {
                stages += 1;
                // Each projection checks all retained nested scopes. Bound the
                // cumulative comparisons and ancestor walks before construction.
                budget.spend(
                    depth
                        .checked_mul(4 * stages + 4)
                        .ok_or(PlanError::AudioQueryLimit("source occurrence graph work"))?,
                )?;
                let stage = AudioStage::for_node(
                    self,
                    edge.parent,
                    &instance.repeats[..edge.repeats],
                    None,
                    None,
                )?;
                let input =
                    projected_input(self, &stage, &provider, placement, support, &mut budget)?;
                // No output gate enters the next voice processor. Current Hold
                // policy is separately evaluated on the final consuming grid.
                let policy = neutral_policy(self, &stage, &mut budget)?;
                let duration = parent.inspection.duration;
                provider =
                    Provider::Projected(AudioStageProjection::new(stage, input, policy, duration)?);
                placement = Placement::IDENTITY;
                support = node_support(self, edge.parent);
            } else {
                budget.spend(1)?;
                placement = edge.map.compose(placement)?;
                let allowed = node_support(self, edge.parent);
                support = intersect(
                    support,
                    placement.local(allowed.start)?..placement.local(allowed.end)?,
                )?;
                if !positive_range(&support)? {
                    return Err(PlanError::InvalidAudioSourceOccurrence(
                        "source occurrence output is not visible",
                    ));
                }
            }
        }
        let extent = placement.range(support.clone())?;
        budget.runs(1 + provider.policy_runs())?;
        let input = AudioSignalTape::new(
            self,
            extent.clone(),
            vec![provider.run(extent.clone(), support)],
        )?;
        let step = ExactRatio::new(
            i128::from(rate.numerator()),
            i128::from(MIX_SAMPLE_RATE) * i128::from(rate.denominator()),
        )?;
        let grid = AudioSampleGrid::<AudioSample>::new(
            ExactRatio::ZERO,
            step,
            AudioBoundaryRule::RoundEven,
        )?;
        let samples = grid.boundary(extent.start)?..grid.boundary(extent.end)?;
        let input = input.remap_policy_window(
            extent.clone(),
            extent.clone(),
            ExactRatio::ZERO,
            step,
            AudioBoundaryRule::RoundEven,
        )?;
        Ok(AudioSourceOccurrence {
            plan: self,
            instance,
            voice,
            extent,
            samples,
            grid,
            input,
            construction_work: budget.maximum - budget.remaining,
            retained_runs: budget.runs,
        })
    }
}

/// Validate the independent recipe even when its owner has no active occurrence
/// in the requested window. A silent query does not make malformed intent valid.
pub(super) fn checked_owner_voice<'plan>(
    plan: &'plan RenderPlan,
    owner: &NodeId,
    recipe: AudioSourceVoiceRecipe,
) -> Result<AudioSourceVoice<'plan>, PlanError> {
    let host = *plan
        .by_id
        .get(owner)
        .ok_or(PlanError::InvalidAudioSourceOccurrence(
            "source occurrence owner is absent",
        ))?;
    checked_voice(
        scoped_signal(plan, host, Vec::new(), node_support(plan, host)),
        recipe,
    )
}

fn checked_voice(
    owner: AudioSignal<'_>,
    recipe: AudioSourceVoiceRecipe,
) -> Result<AudioSourceVoice<'_>, PlanError> {
    let duration = owner.plan.nodes[owner.root].inspection.duration;
    let selected = recipe.mapping.selection_frames_with_offset(
        duration,
        recipe.offset,
        owner.plan.metadata.presentation_basis.frame_rate,
    )?;
    if selected.start.compare_integer(0).is_lt()
        || !selected
            .end
            .checked_sub(selected.start)?
            .compare_integer(0)
            .is_gt()
        || selected.end.compare_integer(duration.frames()).is_gt()
    {
        return Err(PlanError::InvalidAudioSourceOccurrence(
            "source occurrence selection exceeds its owner",
        ));
    }
    owner.source_voice(recipe)
}

fn resolve_edges(
    plan: &RenderPlan,
    host: usize,
    instance: &InstancePath,
    budget: &mut BuildBudget,
) -> Result<(Vec<Edge>, usize), PlanError> {
    let mut chain = vec![host];
    let mut node = host;
    while node != plan.root {
        budget.spend(1)?;
        if chain.len() > MAX_DOCUMENT_DEPTH {
            return Err(PlanError::AudioQueryLimit("source occurrence depth"));
        }
        node = plan.parents[node].ok_or(PlanError::InvalidAudioSourceOccurrence(
            "source occurrence is detached",
        ))?;
        chain.push(node);
    }
    chain.reverse();
    let mut edges = Vec::with_capacity(chain.len().saturating_sub(1));
    let mut repeats = 0usize;
    for pair in chain.windows(2) {
        budget.spend(1)?;
        let parent = pair[0];
        let child = pair[1];
        let scope = repeats;
        let map = match &plan.nodes[parent].kind {
            CompiledKind::Sequence { entries } => {
                budget.spend(entries.len())?;
                let entry = entries.iter().find(|entry| entry.child == child).ok_or(
                    PlanError::InvalidAudioSourceOccurrence(
                        "source occurrence Sequence child is absent",
                    ),
                )?;
                Placement {
                    origin: ExactRatio::integer(entry.start),
                    scale: ExactRatio::ONE,
                }
            }
            CompiledKind::Repeat { layout, .. } => {
                budget.spend(layout.segment_count())?;
                let play = instance
                    .repeats
                    .get(repeats)
                    .filter(|play| play.node == plan.nodes[parent].inspection.id)
                    .and_then(|play| layout.play(&play.iteration))
                    .ok_or(PlanError::InvalidAudioSourceOccurrence(
                        "source occurrence Repeat identity is missing or invalid",
                    ))?;
                let offset = play.branch_offset(&plan.nodes[child].inspection.id).ok_or(
                    PlanError::InvalidAudioSourceOccurrence(
                        "source occurrence is not the selected Repeat branch",
                    ),
                )?;
                repeats += 1;
                Placement {
                    origin: ExactRatio::integer(offset),
                    scale: ExactRatio::ONE,
                }
            }
            CompiledKind::Retime {
                child: expected,
                start,
                scale,
                ..
            } if *expected == child => {
                let reciprocal = ExactRatio::ONE.checked_div(*scale)?;
                Placement {
                    origin: ExactRatio::ZERO.checked_sub(start.checked_mul(reciprocal)?)?,
                    scale: reciprocal,
                }
            }
            _ => {
                return Err(PlanError::InvalidAudioSourceOccurrence(
                    "source occurrence has an invalid structural parent",
                ));
            }
        };
        edges.push(Edge {
            parent,
            map,
            repeats: scope,
        });
    }
    if repeats != instance.repeats.len() {
        return Err(PlanError::InvalidAudioSourceOccurrence(
            "source occurrence has extra Repeat identities",
        ));
    }
    Ok((edges, chain.len()))
}

fn node_support(plan: &RenderPlan, node: usize) -> Range<ExactRatio> {
    ExactRatio::ZERO..ExactRatio::integer(plan.nodes[node].inspection.duration.frames())
}

fn scoped_signal<'plan>(
    plan: &'plan RenderPlan,
    root: usize,
    repeats: Vec<RepeatInstance>,
    support: Range<ExactRatio>,
) -> AudioSignal<'plan> {
    AudioSignal {
        plan,
        root,
        support,
        sampling_support: None,
        constrain_support: false,
        repeats,
        definition: None,
        placed_transform: None,
        bypass_binding: None,
        gap: None,
        provider: SignalProvider::Structural,
    }
}

#[derive(Clone)]
enum Provider<'plan> {
    Source(Box<AudioSignal<'plan>>),
    Projected(Arc<AudioStageProjection<'plan>>),
}

impl<'plan> Provider<'plan> {
    // An intrinsic run retains a regridded copy of its projection's neutral
    // policy as well as the shared projection. This compiler creates exactly
    // one neutral policy run per stage and charges that retained copy too.
    fn policy_runs(&self) -> usize {
        usize::from(matches!(self, Self::Projected(_)))
    }

    fn run(
        &self,
        destination: Range<ExactRatio>,
        source: Range<ExactRatio>,
    ) -> AudioSignalTapeRun<'plan> {
        match self {
            Self::Source(signal) => {
                AudioSignalTapeRun::new(destination, source, signal.as_ref().clone())
            }
            Self::Projected(projection) => {
                AudioSignalTapeRun::intrinsic(destination, source, Arc::clone(projection))
            }
        }
    }
}

fn silence<'plan>(stage: &AudioStage<'plan>) -> AudioSignal<'plan> {
    let mut signal = stage.input_signal();
    signal.provider = SignalProvider::Silence;
    signal
}

fn projected_input<'plan>(
    plan: &'plan RenderPlan,
    stage: &AudioStage<'plan>,
    provider: &Provider<'plan>,
    placement: Placement,
    support: Range<ExactRatio>,
    budget: &mut BuildBudget,
) -> Result<AudioSignalTape<'plan>, PlanError> {
    let selected = stage.descriptor().selection.clone();
    let end = selected.end.checked_sub(selected.start)?;
    let extent = placement.range(support.clone())?;
    let overlap = intersect(extent, selected.clone())?;
    if !positive_range(&overlap)? {
        return Err(PlanError::InvalidAudioSourceOccurrence(
            "source occurrence misses its enclosing Preserve input",
        ));
    }
    let destination =
        overlap.start.checked_sub(selected.start)?..overlap.end.checked_sub(selected.start)?;
    let source = placement.local(overlap.start)?..placement.local(overlap.end)?;
    let mut runs = Vec::with_capacity(3);
    if destination.start != ExactRatio::ZERO {
        runs.push(AudioSignalTapeRun::new(
            ExactRatio::ZERO..destination.start,
            selected.start..overlap.start,
            silence(stage),
        ));
    }
    runs.push(provider.run(destination.clone(), source));
    if destination.end != end {
        runs.push(AudioSignalTapeRun::new(
            destination.end..end,
            overlap.end..selected.end,
            silence(stage),
        ));
    }
    budget.runs(runs.len() + provider.policy_runs())?;
    AudioSignalTape::new(plan, ExactRatio::ZERO..end, runs)
}

fn neutral_policy<'plan>(
    plan: &'plan RenderPlan,
    stage: &AudioStage<'plan>,
    budget: &mut BuildBudget,
) -> Result<AudioSignalTape<'plan>, PlanError> {
    let output = ExactRatio::ZERO..ExactRatio::integer(stage.descriptor().duration.frames());
    budget.runs(1)?;
    AudioSignalTape::new(
        plan,
        output.clone(),
        vec![AudioSignalTapeRun::new(
            output,
            stage.descriptor().selection.clone(),
            silence(stage),
        )],
    )
}

impl<'plan> AudioSignal<'plan> {
    pub(super) fn occurrence_silence_span(
        &self,
        grid: AudioSampleGrid<SignalSample>,
    ) -> Result<AudioSignalSpan<'plan>, PlanError> {
        let transform = self.transform()?;
        let samples = grid.boundary(self.support.start)?..grid.boundary(self.support.end)?;
        Ok(AudioSignalSpan {
            definition: self.definition.clone(),
            samples: samples.clone(),
            allocated_samples: samples.clone(),
            signal_extent: self.support.clone(),
            instance: InstancePath {
                node: self.plan.nodes[self.root].inspection.id.clone(),
                repeats: self.repeats.clone(),
            },
            gap_after: None,
            transform,
            grid,
            sampling: AudioSampleMap::new(
                samples.start,
                transform.local_at(samples.start)?,
                transform
                    .signal_frames_per_sample
                    .checked_div(transform.signal_frames_per_local_frame)?,
            )?,
            retimes: Vec::new(),
            content: AudioSignalContent::Leaf(AudioContent::Silence {
                reason: SilenceReason::NoSourceAudio,
            }),
        })
    }
}

impl<'plan> AudioSourceOccurrence<'plan> {
    pub fn instance(&self) -> &InstancePath {
        &self.instance
    }
    pub fn source(&self) -> &SourceAudio {
        self.voice.source()
    }
    pub fn source_identity(&self) -> AudioSourceVoiceIdentity {
        self.voice.identity()
    }
    pub fn extent(&self) -> Range<ExactRatio> {
        self.extent.clone()
    }
    pub fn samples(&self) -> Range<AudioSample> {
        self.samples.clone()
    }
    pub fn belongs_to(&self, plan: &RenderPlan) -> bool {
        std::ptr::eq(self.plan, plan)
    }
    pub fn construction_work(&self) -> usize {
        self.construction_work
    }

    pub(super) fn retained_runs(&self) -> usize {
        self.retained_runs
    }

    fn check_range(&self, samples: &Range<AudioSample>) -> Result<(), PlanError> {
        if samples.start < self.samples.start
            || samples.end < samples.start
            || samples.end > self.samples.end
        {
            return Err(PlanError::AudioRangeOutOfRange);
        }
        Ok(())
    }

    /// Raw current-voice processing, before current Hold suppression and event
    /// gain/edges. Query narrowing retains the full input and projection graph.
    pub fn processing(
        &self,
        samples: Range<AudioSample>,
        limits: AudioQueryLimits,
    ) -> Result<AudioProcessingQuery<'plan>, PlanError> {
        self.check_range(&samples)?;
        let query = self.input.query(
            SignalSample(samples.start.0)..SignalSample(samples.end.0),
            limits,
        )?;
        let spans = query
            .spans
            .into_iter()
            .map(|span| {
                Ok(AudioProcessingSpan {
                    definition: span.definition,
                    samples: AudioSample(span.samples.start.0)..AudioSample(span.samples.end.0),
                    allocated_samples: AudioSample(span.allocated_samples.start.0)
                        ..AudioSample(span.allocated_samples.end.0),
                    project_extent: span.signal_extent,
                    instance: span.instance,
                    gap_after: span.gap_after,
                    transform: AudioTransform {
                        project_origin: span.transform.signal_origin,
                        project_frames_per_local_frame: span
                            .transform
                            .signal_frames_per_local_frame,
                        project_frames_per_sample: span.transform.signal_frames_per_sample,
                    },
                    grid: self.grid,
                    sampling: AudioSampleMap::new(
                        AudioSample(span.sampling.anchor().0),
                        span.sampling.local_at_anchor(),
                        span.sampling.local_frames_per_sample(),
                    )?,
                    retimes: span.retimes,
                    content: span.content,
                })
            })
            .collect::<Result<Vec<_>, PlanError>>()?;
        Ok(AudioProcessingQuery {
            project_id: query.project_id,
            revision_id: query.revision_id,
            samples,
            spans,
            lookup: query.lookup,
            work: query.work,
        })
    }

    /// Exact current root Hold issuers, independently of retained Original
    /// bindings, source endpoints, creative edges or a future allowance set.
    pub fn hold_policy(
        &self,
        samples: Range<AudioSample>,
        limits: AudioQueryLimits,
    ) -> Result<AudioHoldPolicyQuery<AudioSample>, PlanError> {
        self.check_range(&samples)?;
        self.plan.audio_hold_policy(samples, limits)
    }

    pub fn policy(
        &self,
        samples: Range<AudioSample>,
        limits: AudioQueryLimits,
    ) -> Result<AudioPolicyQuery<AudioSample>, PlanError> {
        let holds = self.hold_policy(samples, limits)?;
        Ok(AudioPolicyQuery {
            suppressed: holds.rules.into_iter().map(|rule| rule.samples).collect(),
            contents: Vec::new(),
            lookup: holds.lookup,
            work: holds.work,
        })
    }
}
