//! Retained sound edits copy the preceding physical sample array. Their frame
//! maps select old integral labels; they never establish a new resampling phase.

use super::*;
use deadpan_plan::{
    AudioRoutedRoot, AudioRoutedRootInput, AudioRoutedSignal, AudioRoutedSignalInput,
    AudioSampleGrid,
};

/// One retained PointCeil sound route before creative edges and current
/// consuming Hold gates. Suppression retains old provider policy and route gaps.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RoutedSignalBlock {
    pub schema_version: u32,
    pub stage: &'static str,
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub allocation: Range<SignalSample>,
    pub start: SignalSample,
    pub samples: Vec<[f32; 2]>,
    pub suppressed: Vec<Range<SignalSample>>,
}

/// One retained RoundEven sound route before creative edges and current
/// consuming Hold gates. Its input retains the complete projected root recipe.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RoutedRootBlock {
    pub schema_version: u32,
    pub stage: &'static str,
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub allocation: Range<AudioSample>,
    pub start: AudioSample,
    pub samples: Vec<[f32; 2]>,
    pub suppressed: Vec<Range<AudioSample>>,
}

pub(super) struct PreparedRoutedRootQuery<'plan> {
    domain: AudioRoutedRoot<'plan>,
    start: AudioSample,
    frames: u32,
    spans: Vec<PreparedRootSpan<'plan>>,
}

struct PreparedRootSpan<'plan> {
    destination: Range<AudioSample>,
    old: Option<Range<i64>>,
    raw: Option<AudioProcessingQuery<'plan>>,
}

impl StageAudio {
    /// Read immutable old provider samples and copy them to the route's current
    /// labels. Every route span shares one work/dependency budget and deadline.
    /// Source input excludes current Hold gates; projected input retains only
    /// its captured processing and output policy. New consuming gates are separate.
    pub fn read_routed_signal(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        domain: &AudioRoutedSignal<'_>,
        start: SignalSample,
        frames: u32,
        timeout: Duration,
        cancelled: &AtomicBool,
    ) -> Result<RoutedSignalBlock, StageAudioError> {
        check_cancel(cancelled)?;
        if !domain.belongs_to(&self.plan) {
            return Err(StageAudioError::ForeignDomain);
        }
        let budget = PreparationBudget::new(timeout, cancelled)?;
        let allocation = domain.samples();
        let end = routed_end(start.0, frames, allocation.start.0..allocation.end.0)?;
        let control = budget.control();
        let query = domain
            .route()
            .query(start..SignalSample(end), control.query_limits()?)?;
        control.spend_plan_work(query.stats.work)?;
        let grid = domain.route().recipe_grid();
        let retained = domain.route().recipe_samples();
        // Convert every lookup before media work. Equal rates alone do not
        // justify rounding a fractional lookup into another physical sample.
        let spans = query
            .spans
            .into_iter()
            .map(|span| {
                let old = span
                    .sampling
                    .map(|sampling| {
                        routed_old_range(
                            sampling.local_at(span.samples.start)?,
                            grid,
                            span.samples.end.0 - span.samples.start.0,
                            retained.start.0..retained.end.0,
                        )
                    })
                    .transpose()?;
                Ok::<_, StageAudioError>((span.samples, old))
            })
            .collect::<Result<Vec<_>, _>>()?;

        // Even an entirely deleted/masked read must admit its retained source
        // or complete preparation. Route silence cannot conceal revocation,
        // unsupported hidden history, or an excessive canonical DSP recipe.
        match domain.input() {
            AudioRoutedSignalInput::Source(voice) => {
                let asset = &voice.source().asset;
                control.admit_dependency(asset)?;
                let source = resolve_source(provider, &self.plan, asset, cancelled)?;
                control.observe(asset, source)?;
            }
            AudioRoutedSignalInput::Projected(projection) => {
                self.preflight_projected(projection, control, 1)?;
                self.prepare_projected(projection, provider, control, 1)?;
            }
        }
        let mut samples = Vec::with_capacity(frames as usize);
        let mut suppressed = Vec::new();
        for (destination, old) in spans {
            control.check()?;
            let count = u32::try_from(destination.end.0 - destination.start.0)
                .map_err(|_| StageAudioError::Range)?;
            let Some(old) = old else {
                samples.extend(vec![[0.0; 2]; count as usize]);
                suppressed.push(destination);
                continue;
            };
            let block = match domain.input() {
                AudioRoutedSignalInput::Source(voice) => self.read_signal(
                    &voice.input_signal(),
                    provider,
                    SignalSample(old.start),
                    count,
                    control,
                    0,
                )?,
                AudioRoutedSignalInput::Projected(projection) => self
                    .read_projected_signal_controlled(
                        projection,
                        provider,
                        SignalSample(old.start),
                        count,
                        control,
                    )?,
            };
            samples.extend(block.samples);
            for range in block.suppressed {
                suppressed.push(
                    SignalSample(shift_label(range.start.0, old.start, destination.start.0)?)
                        ..SignalSample(shift_label(range.end.0, old.start, destination.start.0)?),
                );
            }
        }
        control.check()?;
        Ok(RoutedSignalBlock {
            schema_version: 1,
            stage: "routed_signal_pcm_before_effects_and_current_gates",
            project_id: self.plan.metadata().project_id.clone(),
            revision_id: self.plan.metadata().revision_id.clone(),
            allocation,
            start,
            samples,
            suppressed: merged_suppression(suppressed),
        })
    }

    /// Keep complete original RoundEven source or projected samples at their
    /// retained integral labels. Current consuming gates and fades are separate.
    pub fn read_routed_root(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        domain: &AudioRoutedRoot<'_>,
        start: AudioSample,
        frames: u32,
        timeout: Duration,
        cancelled: &AtomicBool,
    ) -> Result<RoutedRootBlock, StageAudioError> {
        let budget = PreparationBudget::new(timeout, cancelled)?;
        let control = budget.control();
        let prepared = self.preflight_routed_root(domain, start, frames, control)?;
        // Assembly plus the source reader's two transient buffers. Authored bus
        // callers reserve these alongside their Original and f64 accumulator.
        let reservation = u64::from(frames) * 3;
        self.make_room(reservation, false, control)?;
        self.active_frames += reservation;
        let result = self.read_routed_root_controlled(provider, prepared, control);
        self.active_frames -= reservation;
        let block = result?;
        Ok(RoutedRootBlock {
            schema_version: 1,
            stage: "routed_root_pcm_before_effects_and_current_gates",
            project_id: self.plan.metadata().project_id.clone(),
            revision_id: self.plan.metadata().revision_id.clone(),
            allocation: domain.samples(),
            start,
            samples: block.samples,
            suppressed: block.suppressed,
        })
    }

    pub(super) fn preflight_routed_root<'plan>(
        &self,
        domain: &AudioRoutedRoot<'plan>,
        start: AudioSample,
        frames: u32,
        control: WorkControl<'_>,
    ) -> Result<PreparedRoutedRootQuery<'plan>, StageAudioError> {
        control.check()?;
        if !domain.belongs_to(&self.plan) {
            return Err(StageAudioError::ForeignDomain);
        }
        let allocation = domain.samples();
        let end = routed_end(start.0, frames, allocation.start.0..allocation.end.0)?;
        let query = domain
            .route()
            .query(start..AudioSample(end), control.query_limits()?)?;
        control.spend_plan_work(query.stats.work)?;
        match domain.input() {
            AudioRoutedRootInput::Source(root) => control.admit_dependency(&root.source().asset)?,
            AudioRoutedRootInput::Projected(root) => {
                projected_root::projected_root_recipe(
                    root,
                    self.plan.metadata().presentation_basis.frame_rate,
                )?;
                self.preflight_projected(root.projection(), control, 1)?;
            }
        }
        let grid = domain.route().recipe_grid();
        let retained = domain.route().recipe_samples();
        let mut spans = Vec::with_capacity(query.spans.len());
        for span in query.spans {
            let old = span
                .sampling
                .map(|sampling| {
                    routed_old_range(
                        sampling.local_at(span.samples.start)?,
                        grid,
                        span.samples.end.0 - span.samples.start.0,
                        retained.start.0..retained.end.0,
                    )
                })
                .transpose()?;
            let raw = match (domain.input(), &old) {
                (AudioRoutedRootInput::Source(root), Some(old)) => {
                    let query = root.query(
                        AudioSample(old.start)..AudioSample(old.end),
                        control.query_limits()?,
                    )?;
                    control.spend_plan_work(query.work)?;
                    for span in &query.spans {
                        let AudioSignalContent::Leaf(content) = &span.content else {
                            return Err(PlanError::InvalidPlan(
                                "raw root capture contains processing",
                            )
                            .into());
                        };
                        control.preflight_content(content, &self.plan)?;
                    }
                    Some(query)
                }
                (AudioRoutedRootInput::Projected(root), Some(old)) => {
                    let policy = root.policy(
                        AudioSample(old.start)..AudioSample(old.end),
                        control.query_limits()?,
                    )?;
                    control.spend_plan_work(policy.work)?;
                    for content in &policy.contents {
                        control.preflight_content(content, &self.plan)?;
                    }
                    None
                }
                (_, None) => None,
            };
            spans.push(PreparedRootSpan {
                destination: span.samples,
                old,
                raw,
            });
        }
        Ok(PreparedRoutedRootQuery {
            domain: domain.clone(),
            start,
            frames,
            spans,
        })
    }

    pub(super) fn read_routed_root_controlled(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        prepared: PreparedRoutedRootQuery<'_>,
        control: WorkControl<'_>,
    ) -> Result<ReadBlock, StageAudioError> {
        let mut dependencies = Dependencies::new();
        let mut relative_depth = 0;
        // A route Gap or source exhaustion never conceals the retained input.
        match prepared.domain.input() {
            AudioRoutedRootInput::Source(root) => {
                let asset = &root.source().asset;
                let source = resolve_source(provider, &self.plan, asset, control.cancelled)?;
                dependencies.insert(asset.clone(), control.observe(asset, source)?);
            }
            AudioRoutedRootInput::Projected(root) => {
                let input = self.prepare_projected(root.projection(), provider, control, 1)?;
                dependencies.extend(input.dependencies.clone());
                relative_depth = 1 + input.relative_depth;
            }
        }
        let mut samples = Vec::with_capacity(prepared.frames as usize);
        let mut suppressed = Vec::new();
        for span in prepared.spans {
            control.check()?;
            let count = u32::try_from(span.destination.end.0 - span.destination.start.0)
                .map_err(|_| StageAudioError::Range)?;
            let Some(old) = span.old else {
                samples.resize(samples.len() + count as usize, [0.0; 2]);
                suppressed.push(span.destination);
                continue;
            };
            let block = match prepared.domain.input() {
                AudioRoutedRootInput::Source(_) => self.read_root_source_query(
                    provider,
                    span.raw
                        .ok_or(PlanError::InvalidPlan("missing raw root query"))?,
                    control,
                )?,
                AudioRoutedRootInput::Projected(root) => {
                    let block = self.read_projected_root_controlled(
                        provider,
                        root,
                        AudioSample(old.start),
                        count,
                        control,
                    )?;
                    ReadBlock {
                        start: block.start,
                        samples: block.samples,
                        suppressed: block.suppressed,
                        dependencies: Dependencies::new(),
                        relative_depth,
                        exhausted: Vec::new(),
                    }
                }
            };
            samples.extend(block.samples);
            dependencies.extend(block.dependencies);
            for range in block.suppressed {
                suppressed.push(
                    AudioSample(shift_label(
                        range.start.0,
                        old.start,
                        span.destination.start.0,
                    )?)
                        ..AudioSample(shift_label(
                            range.end.0,
                            old.start,
                            span.destination.start.0,
                        )?),
                );
            }
        }
        control.check()?;
        Ok(ReadBlock {
            start: prepared.start,
            samples,
            dependencies,
            relative_depth,
            suppressed: merged_suppression(suppressed),
            exhausted: Vec::new(),
        })
    }

    fn read_root_source_query(
        &self,
        provider: &mut impl AudioSourceProvider,
        query: AudioProcessingQuery<'_>,
        control: WorkControl<'_>,
    ) -> Result<ReadBlock, StageAudioError> {
        let mut samples = Vec::new();
        let mut dependencies = Dependencies::new();
        let mut suppressed = Vec::new();
        for span in query.spans {
            control.check()?;
            let count = u32::try_from(span.samples.end.0 - span.samples.start.0)
                .map_err(|_| StageAudioError::Range)?;
            match &span.content {
                AudioSignalContent::Leaf(AudioContent::Source { source, .. }) => {
                    let asset = &source.asset;
                    let source = resolve_source(provider, &self.plan, asset, control.cancelled)?;
                    dependencies.insert(asset.clone(), control.observe(asset, source)?);
                    let recipe = root_source_recipe(&span, source.index().stream().sample_rate)?;
                    samples.extend(prepare_source_block(
                        source,
                        recipe,
                        span.samples.start,
                        count,
                        control.check()?,
                        control.cancelled,
                    )?);
                }
                AudioSignalContent::Leaf(AudioContent::Silence { .. }) => {
                    samples.resize(samples.len() + count as usize, [0.0; 2]);
                    suppressed.push(span.samples);
                }
                _ => {
                    return Err(
                        PlanError::InvalidPlan("raw root query is not a source recipe").into(),
                    );
                }
            }
        }
        Ok(ReadBlock {
            start: query.samples.start,
            samples,
            dependencies,
            relative_depth: 0,
            suppressed: merged_suppression(suppressed),
            exhausted: Vec::new(),
        })
    }

    fn read_projected_signal_controlled(
        &mut self,
        projection: &AudioStageProjection<'_>,
        provider: &mut impl AudioSourceProvider,
        start: SignalSample,
        frames: u32,
        control: WorkControl<'_>,
    ) -> Result<SignalBlock, StageAudioError> {
        let prepared = self.prepare_projected(projection, provider, control, 1)?;
        let rate = self.plan.metadata().presentation_basis.frame_rate;
        let recipe = stage_recipe(
            prepared.samples.len(),
            AudioSample(0)
                ..AudioSample(
                    i64::try_from(prepared.samples.len()).map_err(|_| TimeError::Overflow)?,
                ),
            ExactRatio::ZERO,
            ExactRatio::ONE.checked_div(samples_per_frame(rate)?)?,
            rate,
        )?;
        let mut samples = sample_prepared(
            &prepared.samples,
            recipe,
            AudioSample(start.0),
            frames,
            control.cancelled,
        )?;
        let suppressed = suppress_signal(
            SignalInput::PreserveTape(projection.output_policy()),
            start,
            &mut samples,
            &self.plan,
            control,
        )?;
        Ok(SignalBlock {
            samples,
            dependencies: prepared.dependencies.clone(),
            suppressed,
            relative_depth: 1 + prepared.relative_depth,
        })
    }
}

fn routed_end(start: i64, frames: u32, allocation: Range<i64>) -> Result<i64, StageAudioError> {
    let end = start
        .checked_add(i64::from(frames))
        .ok_or(StageAudioError::Range)?;
    if frames == 0 || frames > MAX_OUTPUT_FRAMES || start < allocation.start || end > allocation.end
    {
        return Err(StageAudioError::Range);
    }
    Ok(end)
}

fn routed_old_range<S>(
    local: ExactRatio,
    grid: AudioSampleGrid<S>,
    count: i64,
    retained: Range<i64>,
) -> Result<Range<i64>, StageAudioError> {
    let exact = local
        .checked_sub(grid.frame_origin())?
        .checked_div(grid.frames_per_sample())?;
    if exact.denominator() != 1 {
        return Err(
            PlanError::InvalidPlan("retained route lookup is not an integral old sample").into(),
        );
    }
    let start = i64::try_from(exact.numerator()).map_err(|_| TimeError::Overflow)?;
    let end = start.checked_add(count).ok_or(TimeError::Overflow)?;
    if count <= 0 || start < retained.start || end > retained.end {
        return Err(
            PlanError::InvalidPlan("retained route lookup exceeds its complete provider").into(),
        );
    }
    Ok(start..end)
}

fn shift_label(label: i64, old_start: i64, new_start: i64) -> Result<i64, StageAudioError> {
    i64::try_from(i128::from(label) - i128::from(old_start) + i128::from(new_start))
        .map_err(|_| TimeError::Overflow.into())
}
