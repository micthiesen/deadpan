//! Retained sound edits copy the preceding physical sample array. Their frame
//! maps select old integral labels; they never establish a new resampling phase.

use super::*;
use deadpan_plan::{AudioRoutedRoot, AudioRoutedSignal, AudioRoutedSignalInput, AudioSampleGrid};

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

    /// Keep the complete original RoundEven root placement and sample it at
    /// retained integral labels. Route fragments never crop its filter support,
    /// restart Preserve, or reconstruct root phase from final frame endpoints.
    pub fn read_routed_root(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        domain: &AudioRoutedRoot<'_>,
        start: AudioSample,
        frames: u32,
        timeout: Duration,
        cancelled: &AtomicBool,
    ) -> Result<RoutedRootBlock, StageAudioError> {
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
            .query(start..AudioSample(end), control.query_limits()?)?;
        control.spend_plan_work(query.stats.work)?;
        let grid = domain.route().recipe_grid();
        let retained = domain.route().recipe_samples();
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
        let root = domain.root();
        // Validate the complete root recipe even when the queried route is all
        // gaps. The same immutable projection is memoized across every span.
        projected_root::projected_root_recipe(
            root,
            self.plan.metadata().presentation_basis.frame_rate,
        )?;
        self.preflight_projected(root.projection(), control, 1)?;
        self.prepare_projected(root.projection(), provider, control, 1)?;
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
            let block = self.read_projected_root_controlled(
                provider,
                root,
                AudioSample(old.start),
                count,
                control,
            )?;
            samples.extend(block.samples);
            for range in block.suppressed {
                suppressed.push(
                    AudioSample(shift_label(range.start.0, old.start, destination.start.0)?)
                        ..AudioSample(shift_label(range.end.0, old.start, destination.start.0)?),
                );
            }
        }
        control.check()?;
        Ok(RoutedRootBlock {
            schema_version: 1,
            stage: "routed_root_pcm_before_effects_and_current_gates",
            project_id: self.plan.metadata().project_id.clone(),
            revision_id: self.plan.metadata().revision_id.clone(),
            allocation,
            start,
            samples,
            suppressed: merged_suppression(suppressed),
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
