//! Retained sound edits copy the preceding physical sample array. Their frame
//! maps select old integral labels; they never establish a new resampling phase.

use super::*;
use deadpan_plan::{AudioRoutedRoot, AudioRoutedSignal, AudioRoutedSignalInput, AudioSampleGrid};

#[cfg(test)]
#[path = "projected_mask_before_trace.rs"]
mod projected_mask_before_trace;
#[cfg(test)]
#[path = "projected_tape_source_request.rs"]
pub(in crate::stages) mod projected_tape_source_request;
#[cfg(test)]
#[path = "projected_tape_source_trace.rs"]
pub(in crate::stages) mod projected_tape_source_trace;
#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
#[path = "routed_projected_mask_tests.rs"]
pub(super) mod routed_projected_mask_tests;

/// One retained PointCeil sound route before creative edges and current
/// consuming Hold gates. Suppression retains old provider policy and route gaps.
#[derive(Debug, PartialEq, Serialize)]
pub struct RoutedSignalBlock {
    pub schema_version: u32,
    pub stage: &'static str,
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub allocation: Range<SignalSample>,
    pub start: SignalSample,
    pub samples: Vec<[f32; 2]>,
    pub suppressed: SignalSuppressionMask,
}

/// One retained RoundEven sound route before creative edges and current
/// consuming Hold gates. Its input retains the complete projected root recipe.
#[derive(Debug, PartialEq, Serialize)]
pub struct RoutedRootBlock {
    pub schema_version: u32,
    pub stage: &'static str,
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub allocation: Range<AudioSample>,
    pub start: AudioSample,
    pub samples: Vec<[f32; 2]>,
    pub suppressed: AudioSuppressionMask,
}

pub(super) struct PreparedRoutedRootQuery<'plan> {
    pub(super) domain: AudioRoutedRoot<'plan>,
    start: AudioSample,
    frames: u32,
    spans: Vec<PreparedRootSpan<'plan>>,
}

struct PreparedRootSpan<'plan> {
    destination: Range<AudioSample>,
    old: Option<Range<i64>>,
    input: Option<super::retained_sound_input::PreparedRetainedSoundInput<'plan>>,
}

enum RootDelivery<'query, 'plan> {
    Gap(&'query Range<AudioSample>),
    Input {
        old: &'query Range<i64>,
        input: &'query super::retained_sound_input::PreparedRetainedSoundInput<'plan>,
    },
}

impl<'plan> PreparedRootSpan<'plan> {
    fn delivery(&self) -> Result<RootDelivery<'_, 'plan>, StageAudioError> {
        let Some(old) = &self.old else {
            return Ok(RootDelivery::Gap(&self.destination));
        };
        Ok(RootDelivery::Input {
            old,
            input: self
                .input
                .as_ref()
                .ok_or(PlanError::InvalidPlan("missing retained input query"))?,
        })
    }

    fn translate_policy(
        &self,
        old: &Range<i64>,
        range: Range<AudioSample>,
    ) -> Result<Range<AudioSample>, StageAudioError> {
        Ok(AudioSample(shift_label(
            range.start.0,
            old.start,
            self.destination.start.0,
        )?)
            ..AudioSample(shift_label(
                range.end.0,
                old.start,
                self.destination.start.0,
            )?))
    }
}

impl PreparedRoutedRootQuery<'_> {
    /// Probe before allocating cold carrier storage. This inspects only the
    /// sealed input kind, not source content, stored spans or child processing.
    pub(super) fn policy_support(&self) -> policy_ranges::Support {
        super::retained_sound_input::policy_support(self.domain.input())
    }

    /// Visit the prepared delivery map without rebuilding any route/input
    /// query or touching sources/PCM. This returns suppression only: the real
    /// route read does not forward child exhaustion as a separate mask.
    pub(super) fn visit_policy(
        &self,
        control: WorkControl<'_>,
        mut emit: impl FnMut(Range<AudioSample>) -> Result<(), StageAudioError>,
    ) -> Result<policy_ranges::Support, StageAudioError> {
        control.check()?;
        if self.policy_support() == policy_ranges::Support::Unsupported {
            return Ok(policy_ranges::Support::Unsupported);
        }
        for span in &self.spans {
            control.spend_plan_work(1)?;
            match span.delivery()? {
                RootDelivery::Gap(range) => {
                    control.spend_plan_work(1)?;
                    emit(range.clone())?;
                }
                RootDelivery::Input { old, input } => {
                    let support = input.visit_policy(self.domain.input(), control, |range| {
                        control.spend_plan_work(1)?;
                        emit(span.translate_policy(old, range)?)
                    })?;
                    if support != policy_ranges::Support::Complete {
                        return Err(
                            PlanError::InvalidPlan("routed input policy changed kind").into()
                        );
                    }
                }
            }
        }
        control.check()?;
        Ok(policy_ranges::Support::Complete)
    }
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
        let mut suppressed = SignalSuppressionMask::default();
        for (destination, old) in spans {
            control.check()?;
            let count = u32::try_from(destination.end.0 - destination.start.0)
                .map_err(|_| StageAudioError::Range)?;
            let Some(old) = old else {
                samples.extend(vec![[0.0; 2]; count as usize]);
                suppressed.push(destination, &mut self.masks(control))?;
                continue;
            };
            #[cfg(test)]
            let n_call = match domain.input() {
                AudioRoutedSignalInput::Source(_) => Some(read_signal_policy_before_trace::call(
                    self,
                    read_signal_policy_before_trace::Caller::RoutedSource,
                    SignalSample(old.start),
                    count,
                    0,
                )),
                AudioRoutedSignalInput::Projected(_) => None,
            };
            #[cfg(test)]
            let n_native_call = match domain.input() {
                AudioRoutedSignalInput::Source(_) => Some(read_signal_policy_native_trace::call(
                    read_signal_policy_before_trace::Caller::RoutedSource,
                    SignalSample(old.start),
                    count,
                    0,
                )),
                AudioRoutedSignalInput::Projected(_) => None,
            };
            #[cfg(test)]
            let ticket = n_call
                .as_ref()
                .and_then(read_signal_policy_before_trace::CallScope::ticket);
            #[cfg(test)]
            let origin = TraceOrigin::new(
                ticket,
                n_native_call
                    .as_ref()
                    .and_then(read_signal_policy_native_trace::CallScope::ticket),
            );
            let block = match domain.input() {
                #[cfg(not(test))]
                AudioRoutedSignalInput::Source(voice) => self.read_signal(
                    &voice.input_signal(),
                    provider,
                    SignalSample(old.start),
                    count,
                    control,
                    0,
                )?,
                #[cfg(test)]
                AudioRoutedSignalInput::Source(voice) => {
                    let result = self.read_signal(
                        &voice.input_signal(),
                        provider,
                        (SignalSample(old.start), count),
                        control,
                        0,
                        origin,
                    );
                    read_signal_policy_before_trace::block_result(
                        self,
                        control,
                        ticket,
                        "routed_child_result",
                        &result,
                    );
                    result?
                }
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
            #[cfg(not(test))]
            suppressed.extend_try_map(
                block.suppressed,
                |range| {
                    Ok::<_, StageAudioError>(
                        SignalSample(shift_label(range.start.0, old.start, destination.start.0)?)
                            ..SignalSample(shift_label(
                                range.end.0,
                                old.start,
                                destination.start.0,
                            )?),
                    )
                },
                &mut self.masks(control),
            )?;
            #[cfg(test)]
            {
                read_signal_policy_before_trace::mask(
                    self,
                    control,
                    ticket,
                    "routed_child_before_map",
                    &block.suppressed,
                );
                let result = suppressed.extend_try_map(
                    block.suppressed,
                    |range| {
                        Ok::<_, StageAudioError>(
                            SignalSample(shift_label(
                                range.start.0,
                                old.start,
                                destination.start.0,
                            )?)
                                ..SignalSample(shift_label(
                                    range.end.0,
                                    old.start,
                                    destination.start.0,
                                )?),
                        )
                    },
                    &mut self.masks(control),
                );
                read_signal_policy_before_trace::result(
                    self,
                    control,
                    ticket,
                    "routed_map_result",
                    &result,
                );
                read_signal_policy_before_trace::mask(
                    self,
                    control,
                    ticket,
                    "routed_destination_mask",
                    &suppressed,
                );
                result?;
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
            suppressed: suppressed.merge(&self.masks(control))?,
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
        &mut self,
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
        self.preflight_retained_sound_input(domain.input(), control)?;
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
            let input = old
                .as_ref()
                .map(|old| {
                    self.prepare_retained_sound_query(
                        domain.input(),
                        AudioSample(old.start),
                        u32::try_from(old.end - old.start).map_err(|_| StageAudioError::Range)?,
                        control,
                    )
                })
                .transpose()?;
            spans.push(PreparedRootSpan {
                destination: span.samples,
                old,
                input,
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
        self.read_routed_root_borrowed(provider, &prepared, control)
    }

    /// Consume a checked query without moving or cloning its retained graphs.
    /// Source admission and processing work still run for every actual read.
    pub(super) fn read_routed_root_borrowed(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        prepared: &PreparedRoutedRootQuery<'_>,
        control: WorkControl<'_>,
    ) -> Result<ReadBlock, StageAudioError> {
        // A route Gap or source exhaustion never conceals the retained input.
        let (mut dependencies, mut relative_depth) =
            self.admit_retained_sound_input(provider, prepared.domain.input(), control)?;
        let mut samples = Vec::with_capacity(prepared.frames as usize);
        let mut suppressed = AudioSuppressionMask::default();
        for span in &prepared.spans {
            control.check()?;
            let count = u32::try_from(span.destination.end.0 - span.destination.start.0)
                .map_err(|_| StageAudioError::Range)?;
            let (old, input) = match span.delivery()? {
                RootDelivery::Gap(range) => {
                    samples.resize(samples.len() + count as usize, [0.0; 2]);
                    suppressed.push(range.clone(), &mut self.masks(control))?;
                    continue;
                }
                RootDelivery::Input { old, input } => (old, input),
            };
            let block = self.read_retained_sound_query_borrowed(
                provider,
                prepared.domain.input(),
                input,
                control,
            )?;
            samples.extend(block.samples);
            dependencies.extend(block.dependencies);
            relative_depth = relative_depth.max(block.relative_depth);
            suppressed.extend_try_map(
                block.suppressed,
                |range| span.translate_policy(old, range),
                &mut self.masks(control),
            )?;
        }
        control.check()?;
        Ok(ReadBlock {
            start: prepared.start,
            samples,
            dependencies,
            relative_depth,
            suppressed: suppressed.merge(&self.masks(control))?,
            exhausted: Vec::new(),
        })
    }

    pub(super) fn read_root_source_query(
        &self,
        provider: &mut impl AudioSourceProvider,
        query: AudioProcessingQuery<'_>,
        control: WorkControl<'_>,
    ) -> Result<ReadBlock, StageAudioError> {
        self.read_root_source_query_borrowed(provider, &query, control)
    }

    pub(super) fn read_root_source_query_borrowed(
        &self,
        provider: &mut impl AudioSourceProvider,
        query: &AudioProcessingQuery<'_>,
        control: WorkControl<'_>,
    ) -> Result<ReadBlock, StageAudioError> {
        let mut samples = Vec::new();
        let mut dependencies = Dependencies::new();
        let mut suppressed = Vec::new();
        for span in &query.spans {
            control.check()?;
            let count = u32::try_from(span.samples.end.0 - span.samples.start.0)
                .map_err(|_| StageAudioError::Range)?;
            match &span.content {
                AudioSignalContent::Leaf(AudioContent::Source { source, .. }) => {
                    let asset = &source.asset;
                    let source = resolve_source(provider, &self.plan, asset, control.cancelled)?;
                    dependencies.insert(asset.clone(), control.observe(asset, source)?);
                    let recipe = root_source_recipe(span, source.index().stream().sample_rate)?;
                    samples.extend(prepare_source_block(
                        source,
                        recipe,
                        span.samples.start,
                        count,
                        control.check()?,
                        control.cancelled,
                    )?);
                }
                content if super::retained_sound_input::is_raw_silence(content) => {
                    samples.resize(samples.len() + count as usize, [0.0; 2]);
                    suppressed.push(span.samples.clone());
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
            suppressed: AudioSuppressionMask::plain(merged_suppression(suppressed)),
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
        #[cfg(test)]
        let legacy_source = projected_tape_source_request::legacy();
        #[cfg(not(test))]
        let legacy_source = false;
        let suppressed = if legacy_source {
            let suppressed = suppress_signal(
                SignalInput::PreserveTape(projection.output_policy()),
                start,
                &mut samples,
                &self.plan,
                control,
            )?;
            #[cfg(test)]
            projected_mask_before_trace::record(
                self,
                control,
                start,
                frames,
                &prepared,
                &suppressed,
            );
            let suppressed = SignalSuppressionMask::plain(suppressed);
            #[cfg(test)]
            let suppressed = if super::projected_signal_mask_request::legacy() {
                suppressed
            } else {
                suppressed.into_admitted(&mut self.masks(control))?
            };
            #[cfg(not(test))]
            let suppressed = suppressed.into_admitted(&mut self.masks(control))?;
            suppressed
        } else {
            self.suppress_projected_signal_admitted(projection, start, &mut samples, control)?
        };
        Ok(SignalBlock {
            samples,
            dependencies: prepared.dependencies.clone(),
            suppressed,
            relative_depth: 1 + prepared.relative_depth,
        })
    }

    fn suppress_projected_signal_admitted<'work>(
        &mut self,
        projection: &AudioStageProjection<'_>,
        start: SignalSample,
        samples: &mut [[f32; 2]],
        control: WorkControl<'work>,
    ) -> Result<SignalSuppressionMask, StageAudioError> {
        let end = SignalSample(
            start
                .0
                .checked_add(i64::try_from(samples.len()).map_err(|_| TimeError::Overflow)?)
                .ok_or(TimeError::Overflow)?,
        );
        let limits = control.query_limits()?;
        let policy = {
            let mut admission = sound_context_admission::Admission::new(self, control);
            projection.output_policy().policy_after_preserve_admitted(
                start..end,
                limits,
                &mut admission,
            )?
        };
        #[cfg(test)]
        projected_tape_source_trace::query(self, control, &policy);
        let policy = SignalPolicyInput::Admitted(policy);
        if !policy.was_forwarded() {
            control.spend_plan_work(policy.view().work)?;
        }
        for (completed_contents, content) in policy.view().contents.iter().enumerate() {
            #[cfg(not(test))]
            let _ = completed_contents;
            control.preflight_content(content, &self.plan)?;
            #[cfg(test)]
            {
                projected_tape_source_trace::observe(
                    self,
                    control,
                    projected_tape_source_trace::Phase::ContentPreflight {
                        completed: completed_contents + 1,
                    },
                    policy.view(),
                );
            }
        }
        control.check()?;
        #[cfg(test)]
        projected_tape_source_trace::observe(
            self,
            control,
            projected_tape_source_trace::Phase::ControlChecked,
            policy.view(),
        );
        apply_suppression(start, samples, policy.view().suppressed, |sample| sample.0)?;
        #[cfg(test)]
        projected_tape_source_trace::before_handoff(self, control, policy.view());
        #[cfg(test)]
        let _handoff_scope = projected_tape_source_trace::handoff_scope();
        #[cfg(not(test))]
        {
            policy.into_suppression(self, control)
        }
        #[cfg(test)]
        {
            policy.into_suppression(self, control, None)
        }
    }
}

/// The retained voice supplies raw time-mapped input. Current consuming Holds,
/// creative edges and gain belong after routing, not to the captured provider.
pub(super) fn routed_end(
    start: i64,
    frames: u32,
    allocation: Range<i64>,
) -> Result<i64, StageAudioError> {
    let end = start
        .checked_add(i64::from(frames))
        .ok_or(StageAudioError::Range)?;
    if frames == 0 || frames > MAX_OUTPUT_FRAMES || start < allocation.start || end > allocation.end
    {
        return Err(StageAudioError::Range);
    }
    Ok(end)
}

pub(super) fn routed_old_range<S>(
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

#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
#[path = "routed/tests.rs"]
mod tests;
