//! Continuous, bounded time mapping on preparation workers. Each Preserve
//! occurrence owns one canonical history, independent of output queries/crops.
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use deadpan_core::{
    AssetId, AudioSample, ExactRatio, FrameDuration, FrameRate, InstancePath, IterationId,
    ProjectId, RevisionId, SourceAudio, SourceAudioMapping, SourcePoint, TimeError,
};
use deadpan_dsp::{CanonicalRecipe, CanonicalStretch, StereoPcm, StretchRate};
use deadpan_media::audio_index::AudioChannelLayout;
use deadpan_plan::{
    AudioContent, AudioProcessingSpan, AudioQueryLimits, AudioSignal, AudioSignalContent,
    AudioSignalSpan, AudioStage, AudioStageDescriptor, PlanError, RenderPlan, SignalSample,
    SilenceReason,
};
use serde::Serialize;

use crate::sequence::{original_sample, source_samples};
use crate::{
    AudioSourceProvider, MAX_OUTPUT_FRAMES, PcmWindow, PreparationError, ResampleRecipe, Resampler,
    RoomTone, RoomToneRecipe, StereoMatrix, check_cancel,
};

/// PCM residency limits, not a claim about total process memory or latency.
/// Native FFT state, decoder caches and one bounded resampling halo are separate.
#[derive(Debug, Clone, Copy)]
pub struct StageLimits {
    pub maximum_input_frames: u32,
    pub maximum_output_frames: u32,
    pub maximum_resident_frames: u32,
    pub maximum_cached_stages: usize,
    pub maximum_depth: usize,
    pub maximum_prepared_stages: u32,
    pub maximum_prepared_frames: u64,
}

impl Default for StageLimits {
    fn default() -> Self {
        Self {
            maximum_input_frames: deadpan_dsp::MAX_INPUT_FRAMES,
            maximum_output_frames: deadpan_dsp::MAX_OUTPUT_FRAMES,
            maximum_resident_frames: 16 * 1024 * 1024,
            maximum_cached_stages: 64,
            maximum_depth: 32,
            maximum_prepared_stages: 64,
            maximum_prepared_frames: 16 * 1024 * 1024,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StageAudioError {
    #[error("time-mapped inspection requires 1..256 samples inside the sequence")]
    Range,
    #[error("invalid stage preparation limits")]
    InvalidLimits,
    #[error("audio stage preparation exceeds {0}")]
    Limit(&'static str),
    #[error("audio stage preparation deadline expired")]
    Timeout,
    #[error("time-mapped PCM cannot yet render {0}")]
    Unsupported(&'static str),
    #[error(transparent)]
    Preparation(#[from] PreparationError),
    #[error(transparent)]
    Plan(#[from] PlanError),
    #[error(transparent)]
    Time(#[from] TimeError),
    #[error(transparent)]
    Dsp(#[from] deadpan_dsp::DspError),
}

impl StageAudioError {
    pub fn is_cancelled(&self) -> bool {
        matches!(
            self,
            Self::Preparation(PreparationError::Cancelled)
                | Self::Dsp(deadpan_dsp::DspError::Cancelled)
        )
    }
}

/// Before fades, treatments, sends and mastering. Explicit suppression ranges
/// survive for downstream processing; later effects may not fill silent Holds.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TimeMappedBlock {
    pub schema_version: u32,
    pub stage: &'static str,
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub start: AudioSample,
    pub samples: Vec<[f32; 2]>,
    pub suppressed: Vec<Range<AudioSample>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PreparedKey {
    Preserve(AudioStageDescriptor),
    RoomTone {
        instance: InstancePath,
        gap_after: Option<IterationId>,
        source: SourceAudio,
        duration: FrameDuration,
    },
}

struct PreparedStage {
    key: PreparedKey,
    block: SignalBlock,
}

type Dependencies = BTreeMap<AssetId, [u8; 32]>;

#[derive(Default)]
struct SignalBlock {
    samples: Vec<[f32; 2]>,
    dependencies: Dependencies,
}

#[derive(Default)]
struct ReadWork {
    prepared_stages: u32,
    prepared_frames: u64,
    source_checks: u32,
    observed: Dependencies,
}

#[derive(Clone, Copy)]
struct WorkControl<'a> {
    cancelled: &'a AtomicBool,
    deadline: Instant,
    work: &'a RefCell<ReadWork>,
}

impl WorkControl<'_> {
    fn check(self) -> Result<Duration, StageAudioError> {
        check_cancel(self.cancelled)?;
        self.deadline
            .checked_duration_since(Instant::now())
            .filter(|remaining| !remaining.is_zero())
            .ok_or(StageAudioError::Timeout)
    }

    fn observe(
        self,
        asset: &AssetId,
        source: &crate::PreparedSource,
    ) -> Result<[u8; 32], StageAudioError> {
        self.check()?;
        let fingerprint = source.provenance();
        let mut work = self.work.borrow_mut();
        work.source_checks += 1;
        if work.source_checks > 65_536 {
            return Err(StageAudioError::Limit("source provenance checks"));
        }
        if let Some(previous) = work.observed.get(asset) {
            if *previous != fingerprint {
                return Err(PreparationError::IndexMismatch.into());
            }
        } else {
            if work.observed.len() >= 1024 {
                return Err(StageAudioError::Limit("source dependencies"));
            }
            work.observed.insert(asset.clone(), fingerprint);
        }
        Ok(fingerprint)
    }
}

/// One immutable revision and bounded prepared stage PCM. Original sources are
/// supplied by the same qualified revision-aware provider as source inspection.
pub struct StageAudio {
    plan: Arc<RenderPlan>,
    limits: StageLimits,
    cache: Vec<Arc<PreparedStage>>,
    active_frames: u64,
}

impl StageAudio {
    pub fn new(plan: Arc<RenderPlan>) -> Self {
        Self {
            plan,
            limits: StageLimits::default(),
            cache: Vec::new(),
            active_frames: 0,
        }
    }

    pub fn with_limits(
        plan: Arc<RenderPlan>,
        limits: StageLimits,
    ) -> Result<Self, StageAudioError> {
        if limits.maximum_input_frames == 0
            || limits.maximum_input_frames > deadpan_dsp::MAX_INPUT_FRAMES
            || limits.maximum_output_frames == 0
            || limits.maximum_output_frames > deadpan_dsp::MAX_OUTPUT_FRAMES
            || limits.maximum_resident_frames == 0
            || limits.maximum_resident_frames > 16 * 1024 * 1024
            || limits.maximum_cached_stages == 0
            || limits.maximum_cached_stages > 64
            || limits.maximum_depth == 0
            || limits.maximum_depth > 32
            || limits.maximum_prepared_stages == 0
            || limits.maximum_prepared_stages > 64
            || limits.maximum_prepared_frames == 0
            || limits.maximum_prepared_frames > 16 * 1024 * 1024
        {
            return Err(StageAudioError::InvalidLimits);
        }
        Ok(Self {
            limits,
            ..Self::new(plan)
        })
    }

    pub fn plan(&self) -> &RenderPlan {
        &self.plan
    }

    pub fn cached_stage_count(&self) -> usize {
        self.cache.len()
    }

    pub fn read(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        start: AudioSample,
        frames: u32,
        timeout: Duration,
        cancelled: &AtomicBool,
    ) -> Result<TimeMappedBlock, StageAudioError> {
        check_cancel(cancelled)?;
        let end = start
            .0
            .checked_add(i64::from(frames))
            .ok_or(StageAudioError::Range)?;
        if start.0 < 0
            || frames == 0
            || frames > MAX_OUTPUT_FRAMES
            || end > self.plan.audio_duration()?.0
        {
            return Err(StageAudioError::Range);
        }
        if timeout.is_zero() || timeout > Duration::from_secs(60) {
            return Err(PreparationError::InvalidRecipe("audio read time budget").into());
        }
        // A local Arc keeps borrowed stage handles tied to this exact plan
        // without borrowing the mutable cache for the duration of preparation.
        let plan = Arc::clone(&self.plan);
        let work = RefCell::new(ReadWork::default());
        let control = WorkControl {
            cancelled,
            deadline: Instant::now() + timeout,
            work: &work,
        };
        let flattened = plan.audio(start..AudioSample(end), query_limits())?;
        for span in &flattened.spans {
            preflight(&span.content, &plan)?;
        }
        let query = plan.audio_processing(start..AudioSample(end), query_limits())?;
        for span in &query.spans {
            if let AudioSignalContent::Leaf(content) = &span.content {
                preflight(content, &plan)?;
            }
        }
        let mut samples = Vec::with_capacity(frames as usize);
        for span in query.spans {
            control.check()?;
            let block = match &span.content {
                AudioSignalContent::Leaf(AudioContent::Source { source, .. }) => {
                    let prepared = provider.source(
                        &plan.metadata().project_id,
                        &plan.metadata().revision_id,
                        &source.asset,
                        cancelled,
                    )?;
                    control.observe(&source.asset, prepared)?;
                    let recipe = root_source_recipe(&span, prepared.index().stream().sample_rate)?;
                    prepare_source_block(
                        prepared,
                        recipe,
                        span.samples.start,
                        count(&span.samples)?,
                        control.check()?,
                        cancelled,
                    )?
                }
                AudioSignalContent::Leaf(AudioContent::RoomTone { source, duration }) => {
                    let prepared = self.prepare_room_tone(
                        PreparedKey::RoomTone {
                            instance: span.instance.clone(),
                            gap_after: span.gap_after.clone(),
                            source: source.clone(),
                            duration: *duration,
                        },
                        provider,
                        control,
                        1,
                    )?;
                    let recipe = stage_recipe(
                        prepared.block.samples.len(),
                        span.allocated_samples.clone(),
                        span.transform.local_at(span.allocated_samples.start)?,
                        span.transform
                            .project_frames_per_sample
                            .checked_div(span.transform.project_frames_per_local_frame)?,
                        plan.metadata().presentation_basis.frame_rate,
                    )?;
                    sample_prepared(
                        &prepared.block.samples,
                        recipe,
                        span.samples.start,
                        count(&span.samples)?,
                        cancelled,
                    )?
                }
                AudioSignalContent::Leaf(_) => vec![[0.0; 2]; count(&span.samples)? as usize],
                AudioSignalContent::Stage(stage) => {
                    let prepared = self.prepare_stage(stage, provider, control, 1)?;
                    let recipe = stage_recipe(
                        prepared.block.samples.len(),
                        span.allocated_samples.clone(),
                        span.transform.local_at(span.allocated_samples.start)?,
                        span.transform
                            .project_frames_per_sample
                            .checked_div(span.transform.project_frames_per_local_frame)?,
                        plan.metadata().presentation_basis.frame_rate,
                    )?;
                    sample_prepared(
                        &prepared.block.samples,
                        recipe,
                        span.samples.start,
                        count(&span.samples)?,
                        cancelled,
                    )?
                }
            };
            samples.extend(block);
        }
        let mut suppressed = Vec::new();
        for span in flattened.spans {
            if is_silent_hold(&span.content) {
                let left = usize::try_from(span.samples.start.0 - start.0)
                    .map_err(|_| StageAudioError::Range)?;
                let right = usize::try_from(span.samples.end.0 - start.0)
                    .map_err(|_| StageAudioError::Range)?;
                samples[left..right].fill([0.0; 2]);
                suppressed.push(span.samples);
            }
        }
        control.check()?;
        Ok(TimeMappedBlock {
            schema_version: 1,
            stage: "time_mapped_pcm_before_effects",
            project_id: plan.metadata().project_id.clone(),
            revision_id: plan.metadata().revision_id.clone(),
            start,
            samples,
            suppressed,
        })
    }

    fn prepare_stage(
        &mut self,
        stage: &AudioStage<'_>,
        provider: &mut impl AudioSourceProvider,
        control: WorkControl<'_>,
        depth: usize,
    ) -> Result<Arc<PreparedStage>, StageAudioError> {
        control.check()?;
        let key = PreparedKey::Preserve(stage.descriptor().clone());
        if let Some(entry) = self.cached(&key, provider, control)? {
            return Ok(entry);
        }
        if depth > self.limits.maximum_depth {
            return Err(StageAudioError::Limit("nested stage depth"));
        }
        let input_signal = stage.input_signal();
        let output_signal = stage.output_signal();
        let input_frames = u32::try_from(input_signal.sample_count()?.0)
            .map_err(|_| StageAudioError::Limit("input frames"))?;
        let output_frames = u32::try_from(output_signal.sample_count()?.0)
            .map_err(|_| StageAudioError::Limit("output frames"))?;
        let rate = stage.descriptor().rate;
        let recipe = CanonicalRecipe::with_rate(
            input_frames,
            output_frames,
            StretchRate::new(
                u64::try_from(rate.numerator()).map_err(|_| TimeError::Overflow)?,
                u64::try_from(rate.denominator()).map_err(|_| TimeError::Overflow)?,
            )?,
            0,
        );
        let reservation = self.reserve(input_frames, output_frames, control)?;
        let result = recipe.map_err(StageAudioError::from).and_then(|recipe| {
            self.build_stage(
                &input_signal,
                &output_signal,
                recipe,
                provider,
                control,
                depth,
            )
        });
        self.active_frames -= reservation;
        self.publish(key, result?)
    }

    fn cached(
        &mut self,
        key: &PreparedKey,
        provider: &mut impl AudioSourceProvider,
        control: WorkControl<'_>,
    ) -> Result<Option<Arc<PreparedStage>>, StageAudioError> {
        if let Some(index) = self.cache.iter().position(|entry| entry.key == *key) {
            let entry = self.cache.remove(index);
            let mut valid = true;
            for (asset, fingerprint) in &entry.block.dependencies {
                control.check()?;
                let source = provider.source(
                    &self.plan.metadata().project_id,
                    &self.plan.metadata().revision_id,
                    asset,
                    control.cancelled,
                )?;
                valid &= control.observe(asset, source)? == *fingerprint;
            }
            if valid {
                self.cache.push(Arc::clone(&entry));
                return Ok(Some(entry));
            }
        }
        Ok(None)
    }

    fn reserve(
        &mut self,
        input_frames: u32,
        output_frames: u32,
        control: WorkControl<'_>,
    ) -> Result<u64, StageAudioError> {
        if input_frames == 0 || input_frames > self.limits.maximum_input_frames {
            return Err(StageAudioError::Limit("input frames"));
        }
        if output_frames == 0 || output_frames > self.limits.maximum_output_frames {
            return Err(StageAudioError::Limit("output frames"));
        }
        {
            let mut work = control.work.borrow_mut();
            work.prepared_stages += 1;
            work.prepared_frames += u64::from(input_frames) + u64::from(output_frames);
            if work.prepared_stages > self.limits.maximum_prepared_stages {
                return Err(StageAudioError::Limit("prepared stages per read"));
            }
            if work.prepared_frames > self.limits.maximum_prepared_frames {
                return Err(StageAudioError::Limit("prepared frames per read"));
            }
        }
        // Account for interleaved input and its planar conversion simultaneously,
        // plus output. Recursive preparations share the same residency budget.
        let reservation = u64::from(input_frames) * 2 + u64::from(output_frames);
        self.make_room(reservation, false)?;
        self.active_frames += reservation;
        Ok(reservation)
    }

    fn publish(
        &mut self,
        key: PreparedKey,
        block: SignalBlock,
    ) -> Result<Arc<PreparedStage>, StageAudioError> {
        self.make_room(block.samples.len() as u64, true)?;
        let entry = Arc::new(PreparedStage { key, block });
        self.cache.push(Arc::clone(&entry));
        Ok(entry)
    }

    fn prepare_room_tone(
        &mut self,
        key: PreparedKey,
        provider: &mut impl AudioSourceProvider,
        control: WorkControl<'_>,
        depth: usize,
    ) -> Result<Arc<PreparedStage>, StageAudioError> {
        control.check()?;
        if let Some(entry) = self.cached(&key, provider, control)? {
            return Ok(entry);
        }
        if depth > self.limits.maximum_depth {
            return Err(StageAudioError::Limit("nested stage depth"));
        }
        let PreparedKey::RoomTone {
            source, duration, ..
        } = &key
        else {
            return Err(PlanError::InvalidPlan("room tone preparation key").into());
        };
        let source_extent = source_samples(
            SourcePoint {
                ticks: ExactRatio::integer(source.span.end().ticks)
                    .checked_sub(ExactRatio::integer(source.span.start().ticks))?,
                time_base: source.span.start().time_base,
            },
            48_000,
        )?;
        let input_frames = u32::try_from(source_extent.ceil()?)
            .map_err(|_| StageAudioError::Limit("input frames"))?;
        let output_frames = u32::try_from(
            samples_per_frame(self.plan.metadata().presentation_basis.frame_rate)?
                .checked_mul(ExactRatio::integer(duration.frames()))?
                .ceil()?,
        )
        .map_err(|_| StageAudioError::Limit("output frames"))?;
        let reservation = self.reserve(input_frames, output_frames, control)?;
        let result = (|| {
            let recipe = RoomToneRecipe::new(source_extent, output_frames)?;
            let prepared = provider.source(
                &self.plan.metadata().project_id,
                &self.plan.metadata().revision_id,
                &source.asset,
                control.cancelled,
            )?;
            let fingerprint = control.observe(&source.asset, prepared)?;
            let samples = build_room_tone(source, prepared, recipe, input_frames, control)?;
            Ok::<_, StageAudioError>(SignalBlock {
                samples,
                dependencies: BTreeMap::from([(source.asset.clone(), fingerprint)]),
            })
        })();
        self.active_frames -= reservation;
        self.publish(key, result?)
    }

    fn make_room(&mut self, additional: u64, new_entry: bool) -> Result<(), StageAudioError> {
        loop {
            let resident = self
                .cache
                .iter()
                .map(|entry| entry.block.samples.len() as u64)
                .sum::<u64>();
            if resident + self.active_frames + additional
                <= u64::from(self.limits.maximum_resident_frames)
                && (!new_entry || self.cache.len() < self.limits.maximum_cached_stages)
            {
                return Ok(());
            }
            let index = self
                .cache
                .iter()
                .position(|entry| Arc::strong_count(entry) == 1)
                .ok_or(StageAudioError::Limit("resident PCM or stage cache"))?;
            self.cache.remove(index);
        }
    }

    fn build_stage(
        &mut self,
        input: &AudioSignal<'_>,
        output: &AudioSignal<'_>,
        recipe: CanonicalRecipe,
        provider: &mut impl AudioSourceProvider,
        control: WorkControl<'_>,
        depth: usize,
    ) -> Result<SignalBlock, StageAudioError> {
        let WorkControl { cancelled, .. } = control;
        // A policy may own no input-grid points but become audible after the
        // stretch. Validate the complete intrinsic output before source I/O.
        let mut validated = 0_u32;
        while validated < recipe.output_frames() {
            control.check()?;
            let end = (validated + MAX_OUTPUT_FRAMES).min(recipe.output_frames());
            for span in output
                .query_flattened(
                    SignalSample(i64::from(validated))..SignalSample(i64::from(end)),
                    query_limits(),
                )?
                .spans
            {
                if let AudioSignalContent::Leaf(content) = span.content {
                    preflight(&content, &self.plan)?;
                }
            }
            validated = end;
        }
        let mut input_pcm = Vec::with_capacity(recipe.input_frames() as usize);
        let mut dependencies = Dependencies::new();
        while input_pcm.len() < recipe.input_frames() as usize {
            control.check()?;
            let start = SignalSample(input_pcm.len() as i64);
            let frames = (recipe.input_frames() - input_pcm.len() as u32).min(MAX_OUTPUT_FRAMES);
            let block = self.read_signal(input, provider, start, frames, control, depth)?;
            input_pcm.extend(block.samples);
            dependencies.extend(block.dependencies);
        }
        check_cancel(cancelled)?;
        let (left, right) = input_pcm
            .into_iter()
            .map(|frame| (frame[0], frame[1]))
            .unzip();
        let mut dsp = CanonicalStretch::new(recipe, StereoPcm::new(left, right)?)?;
        let mut samples = Vec::with_capacity(recipe.output_frames() as usize);
        while samples.len() < recipe.output_frames() as usize {
            control.check()?;
            let frames =
                (recipe.output_frames() as usize - samples.len()).min(MAX_OUTPUT_FRAMES as usize);
            let mut left = [0.0; MAX_OUTPUT_FRAMES as usize];
            let mut right = [0.0; MAX_OUTPUT_FRAMES as usize];
            let read = dsp.read(&mut left[..frames], &mut right[..frames], cancelled)?;
            if read != frames {
                return Err(deadpan_dsp::DspError::NativeReport.into());
            }
            let mut block = left[..read]
                .iter()
                .zip(&right[..read])
                .map(|(&l, &r)| [l, r])
                .collect::<Vec<_>>();
            suppress_signal(
                output,
                SignalSample(samples.len() as i64),
                &mut block,
                &self.plan,
            )?;
            samples.extend(block);
        }
        control.check()?;
        Ok(SignalBlock {
            samples,
            dependencies,
        })
    }

    fn read_signal(
        &mut self,
        signal: &AudioSignal<'_>,
        provider: &mut impl AudioSourceProvider,
        start: SignalSample,
        frames: u32,
        control: WorkControl<'_>,
        depth: usize,
    ) -> Result<SignalBlock, StageAudioError> {
        let WorkControl { cancelled, .. } = control;
        let query = signal.query(
            start..SignalSample(start.0 + i64::from(frames)),
            query_limits(),
        )?;
        for span in &query.spans {
            if let AudioSignalContent::Leaf(content) = &span.content {
                preflight(content, &self.plan)?;
            }
        }
        let mut samples = Vec::with_capacity(frames as usize);
        let mut dependencies = Dependencies::new();
        for span in query.spans {
            control.check()?;
            let start = AudioSample(span.samples.start.0);
            let count = u32::try_from(span.samples.end.0 - span.samples.start.0)
                .map_err(|_| StageAudioError::Range)?;
            let block = match &span.content {
                AudioSignalContent::Leaf(AudioContent::Source { source, .. }) => {
                    let prepared = provider.source(
                        &query.project_id,
                        &query.revision_id,
                        &source.asset,
                        cancelled,
                    )?;
                    dependencies.insert(
                        source.asset.clone(),
                        control.observe(&source.asset, prepared)?,
                    );
                    let recipe =
                        signal_source_recipe(&span, prepared.index().stream().sample_rate)?;
                    prepare_source_block(
                        prepared,
                        recipe,
                        start,
                        count,
                        control.check()?,
                        cancelled,
                    )?
                }
                AudioSignalContent::Leaf(AudioContent::RoomTone { source, duration }) => {
                    let prepared = self.prepare_room_tone(
                        PreparedKey::RoomTone {
                            instance: span.instance.clone(),
                            gap_after: span.gap_after.clone(),
                            source: source.clone(),
                            duration: *duration,
                        },
                        provider,
                        control,
                        depth + 1,
                    )?;
                    dependencies.extend(prepared.block.dependencies.clone());
                    let recipe = stage_recipe(
                        prepared.block.samples.len(),
                        AudioSample(span.allocated_samples.start.0)
                            ..AudioSample(span.allocated_samples.end.0),
                        span.transform.local_at(span.allocated_samples.start)?,
                        span.transform
                            .signal_frames_per_sample
                            .checked_div(span.transform.signal_frames_per_local_frame)?,
                        self.plan.metadata().presentation_basis.frame_rate,
                    )?;
                    sample_prepared(&prepared.block.samples, recipe, start, count, cancelled)?
                }
                AudioSignalContent::Leaf(_) => vec![[0.0; 2]; count as usize],
                AudioSignalContent::Stage(stage) => {
                    let prepared = self.prepare_stage(stage, provider, control, depth + 1)?;
                    dependencies.extend(prepared.block.dependencies.clone());
                    let recipe = stage_recipe(
                        prepared.block.samples.len(),
                        AudioSample(span.allocated_samples.start.0)
                            ..AudioSample(span.allocated_samples.end.0),
                        span.transform.local_at(span.allocated_samples.start)?,
                        span.transform
                            .signal_frames_per_sample
                            .checked_div(span.transform.signal_frames_per_local_frame)?,
                        self.plan.metadata().presentation_basis.frame_rate,
                    )?;
                    sample_prepared(&prepared.block.samples, recipe, start, count, cancelled)?
                }
            };
            samples.extend(block);
        }
        suppress_signal(signal, start, &mut samples, &self.plan)?;
        Ok(SignalBlock {
            samples,
            dependencies,
        })
    }
}

fn build_room_tone(
    source: &SourceAudio,
    prepared: &crate::PreparedSource,
    recipe: RoomToneRecipe,
    input_frames: u32,
    control: WorkControl<'_>,
) -> Result<Vec<[f32; 2]>, StageAudioError> {
    let rate = prepared.index().stream().sample_rate;
    let selection =
        original_sample(source.span.start(), rate)?..original_sample(source.span.end(), rate)?;
    let source_recipe = ResampleRecipe::new(
        selection.clone(),
        ExactRatio::integer(selection.start),
        AudioSample(0),
        ExactRatio::new(i128::from(rate), 48_000)?,
        AudioSample(0)..AudioSample(i64::from(input_frames)),
    )?;
    let mut input = Vec::with_capacity(input_frames as usize);
    while input.len() < input_frames as usize {
        let frames = (input_frames - input.len() as u32).min(MAX_OUTPUT_FRAMES);
        input.extend(
            prepared
                .prepare(
                    source_recipe.clone(),
                    AudioSample(input.len() as i64),
                    frames,
                    control.check()?,
                    control.cancelled,
                )?
                .samples,
        );
    }
    let output_frames = recipe.output_frames();
    let renderer = RoomTone::new(recipe, &input, control.cancelled)?;
    let mut output = Vec::with_capacity(output_frames as usize);
    while output.len() < output_frames as usize {
        control.check()?;
        let frames = (output_frames - output.len() as u32).min(MAX_OUTPUT_FRAMES);
        output.extend(
            renderer
                .render(AudioSample(output.len() as i64), frames, control.cancelled)?
                .samples,
        );
    }
    control.check()?;
    Ok(output)
}

fn query_limits() -> AudioQueryLimits {
    AudioQueryLimits {
        maximum_spans: MAX_OUTPUT_FRAMES as usize,
        ..Default::default()
    }
}

fn count(samples: &Range<AudioSample>) -> Result<u32, StageAudioError> {
    u32::try_from(samples.end.0 - samples.start.0).map_err(|_| StageAudioError::Range)
}

fn preflight(content: &AudioContent, plan: &RenderPlan) -> Result<(), StageAudioError> {
    match content {
        AudioContent::Silence { .. } => Ok(()),
        AudioContent::RoomTone { .. } => Ok(()),
        AudioContent::Tail { .. } => Err(StageAudioError::Unsupported("effect tails")),
        AudioContent::Source {
            source, duration, ..
        } => {
            if SourceAudioMapping::natural_rate(
                source.span,
                plan.metadata().presentation_basis.frame_rate,
            )?
            .duration_frames(plan.duration())?
                != *duration
            {
                return Err(StageAudioError::Unsupported(
                    "source rate mapping without a pitch policy",
                ));
            }
            Ok(())
        }
    }
}

fn is_silent_hold(content: &AudioContent) -> bool {
    matches!(
        content,
        AudioContent::Silence {
            reason: SilenceReason::SilentHold
        }
    )
}

fn suppress_signal(
    signal: &AudioSignal<'_>,
    start: SignalSample,
    samples: &mut [[f32; 2]],
    plan: &RenderPlan,
) -> Result<(), StageAudioError> {
    let end = SignalSample(
        start
            .0
            .checked_add(i64::try_from(samples.len()).map_err(|_| TimeError::Overflow)?)
            .ok_or(TimeError::Overflow)?,
    );
    for span in signal.query_flattened(start..end, query_limits())?.spans {
        if let AudioSignalContent::Leaf(content) = &span.content {
            preflight(content, plan)?;
            if !is_silent_hold(content) {
                continue;
            }
            let left = usize::try_from(span.samples.start.0 - start.0)
                .map_err(|_| StageAudioError::Range)?;
            let right = usize::try_from(span.samples.end.0 - start.0)
                .map_err(|_| StageAudioError::Range)?;
            samples[left..right].fill([0.0; 2]);
        }
    }
    Ok(())
}

fn root_source_recipe(
    span: &AudioProcessingSpan<'_>,
    rate: u32,
) -> Result<Option<ResampleRecipe>, StageAudioError> {
    let AudioSignalContent::Leaf(AudioContent::Source { source, .. }) = &span.content else {
        return Err(PlanError::NoSourceAudio.into());
    };
    source_recipe(
        source,
        rate,
        span.allocated_samples.clone(),
        span.source_point(span.allocated_samples.start)?,
        span.source_point(AudioSample(
            span.allocated_samples
                .start
                .0
                .checked_add(1)
                .ok_or(TimeError::Overflow)?,
        ))?,
        span.source_point_at_project_frame(span.project_extent.start)?,
        span.source_point_at_project_frame(span.project_extent.end)?,
    )
}

fn signal_source_recipe(
    span: &AudioSignalSpan<'_>,
    rate: u32,
) -> Result<Option<ResampleRecipe>, StageAudioError> {
    let AudioSignalContent::Leaf(AudioContent::Source { source, .. }) = &span.content else {
        return Err(PlanError::NoSourceAudio.into());
    };
    source_recipe(
        source,
        rate,
        AudioSample(span.allocated_samples.start.0)..AudioSample(span.allocated_samples.end.0),
        span.source_point(span.allocated_samples.start)?,
        span.source_point(SignalSample(
            span.allocated_samples
                .start
                .0
                .checked_add(1)
                .ok_or(TimeError::Overflow)?,
        ))?,
        span.source_point_at_signal_frame(span.signal_extent.start)?,
        span.source_point_at_signal_frame(span.signal_extent.end)?,
    )
}

fn source_recipe(
    source: &SourceAudio,
    rate: u32,
    output: Range<AudioSample>,
    origin: SourcePoint,
    next: SourcePoint,
    left: SourcePoint,
    right: SourcePoint,
) -> Result<Option<ResampleRecipe>, StageAudioError> {
    let left = source_samples(left, rate)?
        .ceil()?
        .max(i128::from(original_sample(source.span.start(), rate)?));
    let right = source_samples(right, rate)?
        .ceil()?
        .min(i128::from(original_sample(source.span.end(), rate)?));
    if left >= right {
        return Ok(None);
    }
    let origin = source_samples(origin, rate)?;
    Ok(Some(ResampleRecipe::new(
        i64::try_from(left).map_err(|_| TimeError::Overflow)?
            ..i64::try_from(right).map_err(|_| TimeError::Overflow)?,
        origin,
        output.start,
        source_samples(next, rate)?.checked_sub(origin)?,
        output,
    )?))
}

fn prepare_source_block(
    source: &crate::PreparedSource,
    recipe: Option<ResampleRecipe>,
    start: AudioSample,
    frames: u32,
    timeout: Duration,
    cancelled: &AtomicBool,
) -> Result<Vec<[f32; 2]>, StageAudioError> {
    match recipe {
        Some(recipe) => Ok(source
            .prepare(recipe, start, frames, timeout, cancelled)?
            .samples),
        None => Ok(vec![[0.0; 2]; frames as usize]),
    }
}

fn samples_per_frame(rate: FrameRate) -> Result<ExactRatio, TimeError> {
    ExactRatio::new(
        i128::from(48_000 * u64::from(rate.denominator())),
        i128::from(rate.numerator()),
    )
}

fn stage_recipe(
    length: usize,
    output: Range<AudioSample>,
    local_origin: ExactRatio,
    local_step: ExactRatio,
    rate: FrameRate,
) -> Result<ResampleRecipe, StageAudioError> {
    let conversion = samples_per_frame(rate)?;
    // Prepared-stage context retains the entire intrinsic output. An ancestor
    // crop changes demand, never the established DSP history or kernel context.
    Ok(ResampleRecipe::new(
        0..i64::try_from(length).map_err(|_| TimeError::Overflow)?,
        local_origin.checked_mul(conversion)?,
        output.start,
        local_step.checked_mul(conversion)?,
        output,
    )?)
}

fn sample_prepared(
    samples: &[[f32; 2]],
    recipe: ResampleRecipe,
    start: AudioSample,
    frames: u32,
    cancelled: &AtomicBool,
) -> Result<Vec<[f32; 2]>, StageAudioError> {
    let matrix = StereoMatrix::new(AudioChannelLayout::Native {
        channels: 2,
        mask: 3,
    })?;
    let sampler = Resampler::new(recipe, matrix);
    let window = sampler
        .required_source_range(start, frames)?
        .map(|range| {
            let selected = samples
                .get(range.start as usize..range.end as usize)
                .ok_or(PreparationError::InvalidSamples)?;
            Ok::<_, PreparationError>(PcmWindow {
                start: range.start,
                samples: selected.iter().flat_map(|frame| *frame).collect(),
            })
        })
        .transpose()?;
    Ok(sampler.render(start, frames, window, cancelled)?.samples)
}
