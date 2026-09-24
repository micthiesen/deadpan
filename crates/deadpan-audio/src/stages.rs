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
    AudioBound, AudioBoundDomain, AudioContent, AudioDefinition, AudioDefinitionSelector,
    AudioDomain, AudioFadeQuery, AudioPointDomain, AudioPolicyQuery, AudioProcessingQuery,
    AudioProcessingSpan, AudioQuery, AudioQueryLimits, AudioSignal, AudioSignalContent,
    AudioSignalSpan, AudioStage, AudioStageDescriptor, PlanError, ReferenceSample, RenderPlan,
    SignalSample, SilenceReason,
};
use serde::Serialize;

use crate::sequence::{original_sample, resolve_source, source_samples};
use crate::{
    AudioSourceProvider, DomainSignalTransfer, DomainTransferDescriptor, MAX_OUTPUT_FRAMES,
    PcmWindow, PreparationError, ResampleRecipe, Resampler, RoomTone, RoomToneRecipe,
    RootSignalBlock, RootSignalTransfer, SignalTransferError, StereoMatrix, check_cancel,
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
    #[error("audio domain belongs to another immutable plan")]
    ForeignDomain,
    #[error("audio definition belongs to another immutable plan")]
    ForeignDefinition,
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
    #[error(transparent)]
    Transfer(#[from] SignalTransferError),
}

impl StageAudioError {
    pub fn is_cancelled(&self) -> bool {
        matches!(
            self,
            Self::Preparation(PreparationError::Cancelled)
                | Self::Dsp(deadpan_dsp::DspError::Cancelled)
                | Self::Transfer(SignalTransferError::Preparation(
                    PreparationError::Cancelled
                ))
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

/// Raw PCM from one physical processing domain, before creative fades. Signed
/// indices belong to its captured absolute project grid, including meaningful
/// context outside the domain's visible allocation or the project root itself.
/// Explicit suppression includes both silent Holds and envelope exhaustion.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DomainAudioBlock {
    pub schema_version: u32,
    pub stage: &'static str,
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub definition: Option<AudioDefinitionSelector>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placement: Option<deadpan_plan::AudioRootPlacement>,
    pub instance: InstancePath,
    pub gap_after: Option<IterationId>,
    pub root_samples: Range<AudioSample>,
    pub visible_samples: Range<AudioSample>,
    pub start: AudioSample,
    pub samples: Vec<[f32; 2]>,
    pub suppressed: Vec<Range<AudioSample>>,
}

/// A captured authored definition's raw output on its canonical local-zero
/// point grid. Its selector identifies a definition independently of project
/// occurrences. This is not final timeline allocation.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DefinitionAudioBlock {
    pub schema_version: u32,
    pub stage: &'static str,
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub definition: AudioDefinitionSelector,
    pub root: deadpan_core::NodeId,
    pub start: SignalSample,
    pub samples: Vec<[f32; 2]>,
    pub suppressed: Vec<Range<SignalSample>>,
}

/// Current owned PCM evaluated on the selected PointCeil reference grid.
/// Signed labels retain their original meaning rather than becoming root samples.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PointDomainAudioBlock {
    pub schema_version: u32,
    pub stage: &'static str,
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub definition: AudioDefinitionSelector,
    pub root: deadpan_core::NodeId,
    pub placement: deadpan_plan::AudioRootPlacement,
    pub reference_grid: deadpan_plan::AudioSampleGrid<ReferenceSample>,
    pub reference_samples: Range<ReferenceSample>,
    pub start: ReferenceSample,
    pub samples: Vec<[f32; 2]>,
    pub suppressed: Vec<Range<ReferenceSample>>,
}

/// This revision's raw root signal sampled on an explicitly mapped preparation
/// grid. Root audibility is applied before interpolation and retained afterwards;
/// creative fades and the consuming stage's own policy remain separate.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TransferredRootBlock {
    pub schema_version: u32,
    pub stage: &'static str,
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub transfer: RootSignalTransfer,
    pub start: SignalSample,
    pub samples: Vec<[f32; 2]>,
    pub suppressed: Vec<Range<SignalSample>>,
}

/// A single admitted physical domain transferred onto a preparation grid.
/// Metadata retains the signed captured root clock independently of the output
/// SignalSample grid. Current consuming-stage policies still apply separately.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TransferredDomainBlock {
    pub schema_version: u32,
    pub stage: &'static str,
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub definition: Option<AudioDefinitionSelector>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placement: Option<deadpan_plan::AudioRootPlacement>,
    pub instance: InstancePath,
    pub gap_after: Option<IterationId>,
    pub transfer: DomainTransferDescriptor,
    pub start: SignalSample,
    pub samples: Vec<[f32; 2]>,
    pub suppressed: Vec<Range<SignalSample>>,
}

/// Per-voice edge treatment after continuous time/pitch mapping. This is still
/// before gain, voice effects, sends, mixing and mastering.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EdgeFadedBlock {
    pub schema_version: u32,
    pub stage: &'static str,
    pub engine: &'static str,
    pub processing_order: [&'static str; 2],
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub start: AudioSample,
    pub samples: Vec<[f32; 2]>,
    pub suppressed: Vec<Range<AudioSample>>,
}

struct ReadBlock {
    start: AudioSample,
    samples: Vec<[f32; 2]>,
    // Complete for this block, even if this read already observed the asset or
    // obtained its samples from an admitted preparation cache entry.
    dependencies: Dependencies,
    // Maximum recursive preparation depth below this read's entry depth.
    relative_depth: usize,
    suppressed: Vec<Range<AudioSample>>,
    exhausted: Vec<Range<AudioSample>>,
}

struct RootReadQueries<'plan> {
    flattened: AudioQuery,
    processing: AudioProcessingQuery<'plan>,
    policy: AudioPolicyQuery<AudioSample>,
    fades: Option<AudioFadeQuery>,
}

#[derive(Clone, Copy)]
struct BoundRead {
    offset: i128,
    frames: u32,
    depth: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PreparedKey {
    Preserve(AudioStageDescriptor),
    RoomTone {
        definition: Option<AudioDefinitionSelector>,
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
    suppressed: Vec<Range<SignalSample>>,
    // Retained with cached PCM so reuse at a deeper caller is re-admitted.
    relative_depth: usize,
}

#[derive(Default)]
struct ReadWork {
    prepared_stages: u32,
    prepared_frames: u64,
    source_checks: u32,
    observed: Dependencies,
    plan_work: usize,
}

const MAX_PLAN_WORK_PER_READ: usize = 16 * 1024 * 1024;

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

    fn spend_plan_work(self, amount: usize) -> Result<(), StageAudioError> {
        self.check()?;
        let mut work = self.work.borrow_mut();
        work.plan_work = work
            .plan_work
            .checked_add(amount)
            .filter(|total| *total <= MAX_PLAN_WORK_PER_READ)
            .ok_or(StageAudioError::Limit("plan work per read"))?;
        Ok(())
    }

    fn query_limits(self) -> Result<AudioQueryLimits, StageAudioError> {
        self.check()?;
        let remaining = MAX_PLAN_WORK_PER_READ
            .checked_sub(self.work.borrow().plan_work)
            .filter(|remaining| *remaining > 0)
            .ok_or(StageAudioError::Limit("plan work per read"))?;
        let mut limits = query_limits();
        limits.maximum_work = limits.maximum_work.min(remaining);
        Ok(limits)
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
        let block = self.read_inner(provider, start, frames, timeout, cancelled, false)?;
        Ok(TimeMappedBlock {
            schema_version: 1,
            stage: "time_mapped_pcm_before_effects",
            project_id: self.plan.metadata().project_id.clone(),
            revision_id: self.plan.metadata().revision_id.clone(),
            start: block.start,
            samples: block.samples,
            suppressed: block.suppressed,
        })
    }

    /// Applies one envelope per flattened voice allocation after every mapping
    /// stage. Full allocated endpoints determine gains, so read crops add no
    /// fades and prepared Preserve/room-tone cache history stays untouched.
    pub fn read_edge_faded(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        start: AudioSample,
        frames: u32,
        timeout: Duration,
        cancelled: &AtomicBool,
    ) -> Result<EdgeFadedBlock, StageAudioError> {
        let block = self.read_inner(provider, start, frames, timeout, cancelled, true)?;
        Ok(EdgeFadedBlock {
            schema_version: 1,
            stage: "edge_faded_pcm_before_voice_effects",
            engine: crate::EDGE_FADE_ID,
            processing_order: ["time_pitch_mapping", "edge_fades"],
            project_id: self.plan.metadata().project_id.clone(),
            revision_id: self.plan.metadata().revision_id.clone(),
            start: block.start,
            samples: block.samples,
            suppressed: block.suppressed,
        })
    }

    /// Render a checked authored definition on its own point-ceil output grid.
    /// A Repeat default is read directly even when every actual play overrides
    /// it. Source admission, DSP history and preparation limits remain shared.
    pub fn read_definition(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        definition: &AudioDefinition<'_>,
        start: SignalSample,
        frames: u32,
        timeout: Duration,
        cancelled: &AtomicBool,
    ) -> Result<DefinitionAudioBlock, StageAudioError> {
        check_cancel(cancelled)?;
        if !definition.belongs_to(&self.plan) {
            return Err(StageAudioError::ForeignDefinition);
        }
        validate_timeout(timeout)?;
        let signal = definition.signal();
        let end = start
            .0
            .checked_add(i64::from(frames))
            .ok_or(StageAudioError::Range)?;
        if start.0 < 0
            || frames == 0
            || frames > MAX_OUTPUT_FRAMES
            || end > signal.sample_count()?.0
        {
            return Err(StageAudioError::Range);
        }
        let work = RefCell::new(ReadWork::default());
        let control = WorkControl {
            cancelled,
            deadline: Instant::now() + timeout,
            work: &work,
        };
        let block = self.read_signal(&signal, provider, start, frames, control, 0)?;
        control.check()?;
        Ok(DefinitionAudioBlock {
            schema_version: 1,
            stage: "definition_output_pcm_before_effects",
            project_id: self.plan.metadata().project_id.clone(),
            revision_id: self.plan.metadata().revision_id.clone(),
            definition: definition.selector().clone(),
            root: definition.root().clone(),
            start,
            samples: block.samples,
            suppressed: block.suppressed,
        })
    }

    /// Evaluate a live owned physical recipe on its explicitly selected point
    /// clock. Point labels and allocation remain distinct from root audio.
    pub fn read_point_domain(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        domain: &AudioPointDomain<'_>,
        start: ReferenceSample,
        frames: u32,
        timeout: Duration,
        cancelled: &AtomicBool,
    ) -> Result<PointDomainAudioBlock, StageAudioError> {
        check_cancel(cancelled)?;
        validate_timeout(timeout)?;
        let work = RefCell::new(ReadWork::default());
        let block = self.read_point_domain_controlled(
            provider,
            domain,
            start,
            frames,
            WorkControl {
                cancelled,
                deadline: Instant::now() + timeout,
                work: &work,
            },
            0,
        )?;
        let suppressed = block
            .suppressed
            .into_iter()
            .map(|range| {
                Ok::<_, PlanError>(
                    domain.reference_at_signal(range.start)?
                        ..domain.reference_at_signal(range.end)?,
                )
            })
            .collect::<Result<_, _>>()?;
        Ok(PointDomainAudioBlock {
            schema_version: 1,
            stage: "owned_point_domain_pcm_before_effects",
            project_id: self.plan.metadata().project_id.clone(),
            revision_id: self.plan.metadata().revision_id.clone(),
            definition: domain.definition().clone(),
            root: domain.root().clone(),
            placement: domain.placement().clone(),
            reference_grid: domain.reference_grid(),
            reference_samples: domain.reference_samples(),
            start,
            samples: block.samples,
            suppressed,
        })
    }

    // The signal retains the exact placed support and selected grid origin.
    // Future bound consumers reuse this controller and preserve descendant
    // evaluation scope rather than re-entering a public preparation boundary.
    fn read_point_domain_controlled(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        domain: &AudioPointDomain<'_>,
        start: ReferenceSample,
        frames: u32,
        control: WorkControl<'_>,
        depth: usize,
    ) -> Result<SignalBlock, StageAudioError> {
        control.check()?;
        if !domain.belongs_to(&self.plan) {
            return Err(StageAudioError::ForeignDomain);
        }
        if depth > self.limits.maximum_depth {
            return Err(StageAudioError::Limit("nested stage depth"));
        }
        if frames == 0 || frames > MAX_OUTPUT_FRAMES {
            return Err(StageAudioError::Range);
        }
        let end = ReferenceSample(
            start
                .0
                .checked_add(i64::from(frames))
                .ok_or(StageAudioError::Range)?,
        );
        let signal_start = domain.signal_at_reference(start)?;
        domain.signal_at_reference(end)?;
        let block = self.read_signal(
            &domain.signal(),
            provider,
            signal_start,
            frames,
            control,
            depth,
        )?;
        control.check()?;
        Ok(block)
    }

    /// Render this plan's complete physical processing context, independently
    /// of visible Partition allocation. Never resolve these signed positions
    /// through the project root, where another sibling may own the same sample.
    pub fn read_domain(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        domain: &AudioDomain<'_>,
        start: AudioSample,
        frames: u32,
        timeout: Duration,
        cancelled: &AtomicBool,
    ) -> Result<DomainAudioBlock, StageAudioError> {
        check_cancel(cancelled)?;
        validate_timeout(timeout)?;
        let work = RefCell::new(ReadWork::default());
        let mut block = self.read_domain_controlled(
            provider,
            domain,
            start,
            frames,
            WorkControl {
                cancelled,
                deadline: Instant::now() + timeout,
                work: &work,
            },
            0,
        )?;
        block.suppressed.append(&mut block.exhausted);
        Ok(DomainAudioBlock {
            schema_version: 1,
            stage: "physical_domain_pcm_before_effects",
            project_id: self.plan.metadata().project_id.clone(),
            revision_id: self.plan.metadata().revision_id.clone(),
            definition: domain.definition().cloned(),
            placement: domain.placement().cloned(),
            instance: domain.instance().clone(),
            gap_after: domain.gap_after().cloned(),
            root_samples: domain.root_samples(),
            visible_samples: domain.visible_samples(),
            start: block.start,
            samples: block.samples,
            suppressed: merged_suppression(block.suppressed),
        })
    }

    fn read_domain_controlled(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        domain: &AudioDomain<'_>,
        start: AudioSample,
        frames: u32,
        control: WorkControl<'_>,
        depth: usize,
    ) -> Result<ReadBlock, StageAudioError> {
        control.check()?;
        if !domain.belongs_to(&self.plan) {
            return Err(StageAudioError::ForeignDomain);
        }
        let end = start
            .0
            .checked_add(i64::from(frames))
            .map(AudioSample)
            .ok_or(StageAudioError::Range)?;
        if frames == 0 || frames > MAX_OUTPUT_FRAMES {
            return Err(StageAudioError::Range);
        }
        let flattened = domain.audio(start..end, control.query_limits()?)?;
        control.spend_plan_work(flattened.work)?;
        let processing = domain.processing(start..end, control.query_limits()?)?;
        control.spend_plan_work(processing.work)?;
        let policy = domain.policy(start..end, control.query_limits()?)?;
        control.spend_plan_work(policy.work)?;
        let queries = RootReadQueries {
            flattened,
            processing,
            policy,
            fades: None,
        };
        self.read_queries(provider, start, frames, control, depth, queries)
    }

    /// Transfer hidden or visible domain PCM with one preparation allowance and
    /// deadline across all interpolation halo reads. The transfer retains the
    /// borrowed domain; foreign plans cannot supply a same-named replacement.
    pub fn read_domain_transferred(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        transfer: &DomainSignalTransfer<'_>,
        start: SignalSample,
        frames: u32,
        timeout: Duration,
        cancelled: &AtomicBool,
    ) -> Result<TransferredDomainBlock, StageAudioError> {
        check_cancel(cancelled)?;
        if !transfer.domain().belongs_to(&self.plan) {
            return Err(StageAudioError::ForeignDomain);
        }
        validate_timeout(timeout)?;
        let work = RefCell::new(ReadWork::default());
        let control = WorkControl {
            cancelled,
            deadline: Instant::now() + timeout,
            work: &work,
        };
        let block =
            self.read_domain_transferred_controlled(provider, transfer, start, frames, control, 0)?;
        Ok(TransferredDomainBlock {
            schema_version: 1,
            stage: "physical_domain_on_point_grid_before_effects",
            project_id: self.plan.metadata().project_id.clone(),
            revision_id: self.plan.metadata().revision_id.clone(),
            definition: transfer.domain().definition().cloned(),
            placement: transfer.domain().placement().cloned(),
            instance: transfer.domain().instance().clone(),
            gap_after: transfer.domain().gap_after().cloned(),
            transfer: transfer.descriptor().clone(),
            start,
            samples: block.samples,
            suppressed: block.suppressed,
        })
    }

    // Recursive consumers retain their parent's deadline, work allowance and
    // depth. The returned dependencies cover every halo, including cache hits.
    fn read_domain_transferred_controlled(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        transfer: &DomainSignalTransfer<'_>,
        start: SignalSample,
        frames: u32,
        control: WorkControl<'_>,
        depth: usize,
    ) -> Result<SignalBlock, StageAudioError> {
        control.check()?;
        if depth > self.limits.maximum_depth {
            return Err(StageAudioError::Limit("nested stage depth"));
        }
        if !transfer.domain().belongs_to(&self.plan) {
            return Err(StageAudioError::ForeignDomain);
        }
        let mut dependencies = Dependencies::new();
        let mut relative_depth = 0;
        let anchor = transfer.descriptor().root_support.start.0;
        let block = transfer.carrier().render::<StageAudioError>(
            start,
            frames,
            control.cancelled,
            |at, count| {
                // `at` indexes a zero-based carrier of already allocated root
                // samples. Restore the integer label, not a rounded frame origin.
                let absolute = AudioSample(at.0.checked_add(anchor).ok_or(TimeError::Overflow)?);
                let mut block = self.read_domain_controlled(
                    provider,
                    transfer.domain(),
                    absolute,
                    count,
                    control,
                    depth,
                )?;
                dependencies.extend(block.dependencies);
                relative_depth = relative_depth.max(block.relative_depth);
                block.suppressed.append(&mut block.exhausted);
                let suppressed = block
                    .suppressed
                    .into_iter()
                    .map(|range| {
                        Ok::<_, TimeError>(
                            AudioSample(
                                range
                                    .start
                                    .0
                                    .checked_sub(anchor)
                                    .ok_or(TimeError::Overflow)?,
                            )
                                ..AudioSample(
                                    range.end.0.checked_sub(anchor).ok_or(TimeError::Overflow)?,
                                ),
                        )
                    })
                    .collect::<Result<_, _>>()?;
                Ok(RootSignalBlock {
                    start: at,
                    samples: block.samples,
                    suppressed,
                })
            },
        )?;
        control.check()?;
        Ok(SignalBlock {
            samples: block.samples,
            dependencies,
            suppressed: block.suppressed,
            relative_depth,
        })
    }

    /// Read a bound physical recipe with this read's admission and work state.
    /// Output labels are rebased from the containing span, never rerounded.
    fn read_bound(
        &mut self,
        bound: &AudioBound<'_>,
        provider: &mut impl AudioSourceProvider,
        request: BoundRead,
        control: WorkControl<'_>,
    ) -> Result<SignalBlock, StageAudioError> {
        let BoundRead {
            offset,
            frames,
            depth,
        } = request;
        control.check()?;
        if depth > self.limits.maximum_depth {
            return Err(StageAudioError::Limit("nested stage depth"));
        }
        if !bound.belongs_to(&self.plan) {
            return Err(StageAudioError::ForeignDomain);
        }
        if offset < 0 || frames == 0 || frames > MAX_OUTPUT_FRAMES {
            return Err(StageAudioError::Range);
        }
        let at = bound.reference_at_wide_offset(offset)?;
        let step = bound.reference_samples_per_output_sample();
        let output = SignalSample(0)..SignalSample(i64::from(frames));
        match bound.raw_domain()? {
            AudioBoundDomain::Root(domain) => {
                if domain.root_samples().is_empty() {
                    return Ok(empty_bound_block(frames));
                }
                let transfer = DomainSignalTransfer::new(domain, at, output.start, step, output)?;
                self.read_domain_transferred_controlled(
                    provider,
                    &transfer,
                    SignalSample(0),
                    frames,
                    control,
                    depth,
                )
            }
            AudioBoundDomain::Point(domain) => {
                let support = domain.reference_samples();
                let length = support
                    .end
                    .0
                    .checked_sub(support.start.0)
                    .ok_or(TimeError::Overflow)?;
                if length == 0 {
                    return Ok(empty_bound_block(frames));
                }
                let transfer = RootSignalTransfer::new(
                    AudioSample(0)..AudioSample(length),
                    at.checked_sub(ExactRatio::integer(support.start.0))?,
                    output.start,
                    step,
                    output,
                )?;
                let mut dependencies = Dependencies::new();
                let mut relative_depth = 0;
                let block = transfer.render::<StageAudioError>(
                    SignalSample(0),
                    frames,
                    control.cancelled,
                    |storage, count| {
                        let reference = domain.reference_at_signal(SignalSample(storage.0))?;
                        let block = self.read_point_domain_controlled(
                            provider, &domain, reference, count, control, depth,
                        )?;
                        dependencies.extend(block.dependencies);
                        relative_depth = relative_depth.max(block.relative_depth);
                        // These labels index the already rebased carrier. The
                        // PointCeil reference origin remains in `domain`.
                        Ok(RootSignalBlock {
                            start: storage,
                            samples: block.samples,
                            suppressed: block
                                .suppressed
                                .into_iter()
                                .map(|range| AudioSample(range.start.0)..AudioSample(range.end.0))
                                .collect(),
                        })
                    },
                )?;
                control.check()?;
                Ok(SignalBlock {
                    samples: block.samples,
                    dependencies,
                    suppressed: block.suppressed,
                    relative_depth,
                })
            }
            AudioBoundDomain::Empty => Ok(empty_bound_block(frames)),
        }
    }

    /// Convert the complete root signal into a point-grid input, using one work
    /// allowance and deadline across every halo read. `transfer` describes sample
    /// coordinates only; this renderer owns the immutable media/revision context.
    /// Its support must retain the full root, even for a cropped output request.
    /// This does not author continuity bindings or run a new Preserve stage.
    pub fn read_transferred(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        transfer: &RootSignalTransfer,
        start: SignalSample,
        frames: u32,
        timeout: Duration,
        cancelled: &AtomicBool,
    ) -> Result<TransferredRootBlock, StageAudioError> {
        check_cancel(cancelled)?;
        if transfer.root_support() != (AudioSample(0)..self.plan.audio_duration()?) {
            return Err(StageAudioError::Range);
        }
        validate_timeout(timeout)?;
        let work = RefCell::new(ReadWork::default());
        let control = WorkControl {
            cancelled,
            deadline: Instant::now() + timeout,
            work: &work,
        };
        let block = transfer.render::<StageAudioError>(start, frames, cancelled, |at, count| {
            let mut block = self.read_controlled(provider, at, count, control, false, 0)?;
            block.suppressed.append(&mut block.exhausted);
            Ok(RootSignalBlock {
                start: block.start,
                samples: block.samples,
                suppressed: block.suppressed,
            })
        })?;
        control.check()?;
        Ok(TransferredRootBlock {
            schema_version: 1,
            stage: "root_signal_on_point_grid_before_effects",
            project_id: self.plan.metadata().project_id.clone(),
            revision_id: self.plan.metadata().revision_id.clone(),
            transfer: transfer.clone(),
            start: block.start,
            samples: block.samples,
            suppressed: block.suppressed,
        })
    }

    fn read_inner(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        start: AudioSample,
        frames: u32,
        timeout: Duration,
        cancelled: &AtomicBool,
        edge_fades: bool,
    ) -> Result<ReadBlock, StageAudioError> {
        check_cancel(cancelled)?;
        validate_timeout(timeout)?;
        let work = RefCell::new(ReadWork::default());
        self.read_controlled(
            provider,
            start,
            frames,
            WorkControl {
                cancelled,
                deadline: Instant::now() + timeout,
                work: &work,
            },
            edge_fades,
            0,
        )
    }

    fn read_controlled(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        start: AudioSample,
        frames: u32,
        control: WorkControl<'_>,
        edge_fades: bool,
        depth: usize,
    ) -> Result<ReadBlock, StageAudioError> {
        control.check()?;
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
        // A local Arc keeps borrowed stage handles tied to this exact plan
        // without borrowing the mutable cache for the duration of preparation.
        let plan = Arc::clone(&self.plan);
        let flattened = plan.audio(start..AudioSample(end), control.query_limits()?)?;
        control.spend_plan_work(flattened.work)?;
        let processing = plan.audio_processing(start..AudioSample(end), control.query_limits()?)?;
        control.spend_plan_work(processing.work)?;
        let policy = plan.audio_policy(start..AudioSample(end), control.query_limits()?)?;
        control.spend_plan_work(policy.work)?;
        let fades = if edge_fades {
            let query = plan.audio_fades(start..AudioSample(end), control.query_limits()?)?;
            control.spend_plan_work(query.work)?;
            Some(query)
        } else {
            None
        };
        let queries = RootReadQueries {
            flattened,
            processing,
            policy,
            fades,
        };
        self.read_queries(provider, start, frames, control, depth, queries)
    }

    fn read_queries(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        start: AudioSample,
        frames: u32,
        control: WorkControl<'_>,
        depth: usize,
        queries: RootReadQueries<'_>,
    ) -> Result<ReadBlock, StageAudioError> {
        control.check()?;
        if depth > self.limits.maximum_depth {
            return Err(StageAudioError::Limit("nested stage depth"));
        }
        let cancelled = control.cancelled;
        let plan = Arc::clone(&self.plan);
        if let Some(fades) = &queries.fades {
            crate::edges::validate_creative_fades(start, frames as usize, &fades.spans)?;
        }
        for span in &queries.flattened.spans {
            preflight(&span.content, &plan)?;
        }
        for span in &queries.processing.spans {
            if let AudioSignalContent::Leaf(content) = &span.content {
                preflight(content, &plan)?;
            }
        }
        for content in &queries.policy.contents {
            preflight(content, &plan)?;
        }
        let mut samples = Vec::with_capacity(frames as usize);
        let mut dependencies = Dependencies::new();
        let mut relative_depth = 0;
        for span in queries.processing.spans {
            control.check()?;
            let block = match &span.content {
                AudioSignalContent::Leaf(AudioContent::Source { source, .. }) => {
                    let prepared = resolve_source(provider, &plan, &source.asset, cancelled)?;
                    dependencies.insert(
                        source.asset.clone(),
                        control.observe(&source.asset, prepared)?,
                    );
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
                            definition: span.definition.clone(),
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
                    relative_depth = relative_depth.max(1 + prepared.block.relative_depth);
                    let recipe = root_stage_recipe(
                        &span,
                        prepared.block.samples.len(),
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
                    let prepared = self.prepare_stage(stage, provider, control, depth + 1)?;
                    dependencies.extend(prepared.block.dependencies.clone());
                    relative_depth = relative_depth.max(1 + prepared.block.relative_depth);
                    let recipe = root_stage_recipe(
                        &span,
                        prepared.block.samples.len(),
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
                AudioSignalContent::Bound(bound) => {
                    let block = self.read_bound(
                        bound,
                        provider,
                        BoundRead {
                            offset: i128::from(span.samples.start.0)
                                - i128::from(span.allocated_samples.start.0),
                            frames: count(&span.samples)?,
                            depth: depth + 1,
                        },
                        control,
                    )?;
                    dependencies.extend(block.dependencies);
                    relative_depth = relative_depth.max(1 + block.relative_depth);
                    block.samples
                }
            };
            samples.extend(block);
        }
        let mut suppressed = Vec::new();
        let mut exhausted = Vec::new();
        for span in queries.flattened.spans {
            control.check()?;
            let left = usize::try_from(span.samples.start.0 - start.0)
                .map_err(|_| StageAudioError::Range)?;
            let right = usize::try_from(span.samples.end.0 - start.0)
                .map_err(|_| StageAudioError::Range)?;
            if is_silent_hold(&span.content) {
                samples[left..right].fill([0.0; 2]);
                suppressed.push(span.samples);
            } else {
                crate::edges::apply_retained_envelope(&span, &mut samples[left..right], false)?;
                exhausted.extend(crate::edges::exhausted_ranges(&span)?);
            }
        }
        apply_suppression(start, &mut samples, &queries.policy.suppressed, |sample| {
            sample.0
        })?;
        suppressed.extend(queries.policy.suppressed);
        if let Some(fades) = &queries.fades {
            crate::edges::apply_creative_fades(start, &mut samples, &fades.spans)?;
        }
        control.check()?;
        Ok(ReadBlock {
            start,
            samples,
            dependencies,
            relative_depth,
            suppressed: merged_suppression(suppressed),
            exhausted,
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
        if depth > self.limits.maximum_depth {
            return Err(StageAudioError::Limit("nested stage depth"));
        }
        let key = PreparedKey::Preserve(stage.descriptor().clone());
        if let Some(entry) = self.cached(&key, provider, control, depth)? {
            return Ok(entry);
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
        depth: usize,
    ) -> Result<Option<Arc<PreparedStage>>, StageAudioError> {
        if let Some(index) = self.cache.iter().position(|entry| entry.key == *key) {
            if depth
                .checked_add(self.cache[index].block.relative_depth)
                .is_none_or(|maximum| maximum > self.limits.maximum_depth)
            {
                return Err(StageAudioError::Limit("nested stage depth"));
            }
            let entry = self.cache.remove(index);
            let mut valid = true;
            for (asset, fingerprint) in &entry.block.dependencies {
                control.check()?;
                let source = resolve_source(provider, &self.plan, asset, control.cancelled)?;
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
        if depth > self.limits.maximum_depth {
            return Err(StageAudioError::Limit("nested stage depth"));
        }
        if let Some(entry) = self.cached(&key, provider, control, depth)? {
            return Ok(entry);
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
            let prepared = resolve_source(provider, &self.plan, &source.asset, control.cancelled)?;
            let fingerprint = control.observe(&source.asset, prepared)?;
            let samples = build_room_tone(source, prepared, recipe, input_frames, control)?;
            Ok::<_, StageAudioError>(SignalBlock {
                samples,
                dependencies: BTreeMap::from([(source.asset.clone(), fingerprint)]),
                suppressed: Vec::new(),
                relative_depth: 0,
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
            let policy = output.policy(
                SignalSample(i64::from(validated))..SignalSample(i64::from(end)),
                control.query_limits()?,
            )?;
            control.spend_plan_work(policy.work)?;
            for content in &policy.contents {
                preflight(content, &self.plan)?;
            }
            validated = end;
        }
        let mut input_pcm = Vec::with_capacity(recipe.input_frames() as usize);
        let mut dependencies = Dependencies::new();
        let mut relative_depth = 0;
        while input_pcm.len() < recipe.input_frames() as usize {
            control.check()?;
            let start = SignalSample(input_pcm.len() as i64);
            let frames = (recipe.input_frames() - input_pcm.len() as u32).min(MAX_OUTPUT_FRAMES);
            let block = self.read_signal(input, provider, start, frames, control, depth)?;
            input_pcm.extend(block.samples);
            dependencies.extend(block.dependencies);
            relative_depth = relative_depth.max(block.relative_depth);
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
                control,
            )?;
            samples.extend(block);
        }
        control.check()?;
        Ok(SignalBlock {
            samples,
            dependencies,
            // Consumers query policy in their own point grid. Do not retain a
            // second full-stage interval cache or scale rounded input masks.
            suppressed: Vec::new(),
            relative_depth,
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
        control.check()?;
        if depth > self.limits.maximum_depth {
            return Err(StageAudioError::Limit("nested stage depth"));
        }
        let query = signal.query(
            start..SignalSample(start.0 + i64::from(frames)),
            control.query_limits()?,
        )?;
        control.spend_plan_work(query.work)?;
        let policy = signal.policy(
            start..SignalSample(start.0 + i64::from(frames)),
            control.query_limits()?,
        )?;
        control.spend_plan_work(policy.work)?;
        for content in &policy.contents {
            preflight(content, &self.plan)?;
        }
        for span in &query.spans {
            if let AudioSignalContent::Leaf(content) = &span.content {
                preflight(content, &self.plan)?;
            }
        }
        let mut samples = Vec::with_capacity(frames as usize);
        let mut dependencies = Dependencies::new();
        let mut relative_depth = 0;
        for span in query.spans {
            control.check()?;
            let start = AudioSample(span.samples.start.0);
            let count = u32::try_from(span.samples.end.0 - span.samples.start.0)
                .map_err(|_| StageAudioError::Range)?;
            let block = match &span.content {
                AudioSignalContent::Leaf(AudioContent::Source { source, .. }) => {
                    let prepared = resolve_source(provider, &self.plan, &source.asset, cancelled)?;
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
                            definition: signal.definition().cloned(),
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
                    relative_depth = relative_depth.max(1 + prepared.block.relative_depth);
                    let recipe = signal_stage_recipe(
                        &span,
                        prepared.block.samples.len(),
                        self.plan.metadata().presentation_basis.frame_rate,
                    )?;
                    sample_prepared(&prepared.block.samples, recipe, start, count, cancelled)?
                }
                AudioSignalContent::Leaf(_) => vec![[0.0; 2]; count as usize],
                AudioSignalContent::Stage(stage) => {
                    let prepared = self.prepare_stage(stage, provider, control, depth + 1)?;
                    dependencies.extend(prepared.block.dependencies.clone());
                    relative_depth = relative_depth.max(1 + prepared.block.relative_depth);
                    let recipe = signal_stage_recipe(
                        &span,
                        prepared.block.samples.len(),
                        self.plan.metadata().presentation_basis.frame_rate,
                    )?;
                    sample_prepared(&prepared.block.samples, recipe, start, count, cancelled)?
                }
                AudioSignalContent::Bound(bound) => {
                    let block = self.read_bound(
                        bound,
                        provider,
                        BoundRead {
                            offset: i128::from(span.samples.start.0)
                                - i128::from(span.allocated_samples.start.0),
                            frames: count,
                            depth: depth + 1,
                        },
                        control,
                    )?;
                    dependencies.extend(block.dependencies);
                    relative_depth = relative_depth.max(1 + block.relative_depth);
                    block.samples
                }
            };
            samples.extend(block);
        }
        control.check()?;
        apply_suppression(start, &mut samples, &policy.suppressed, |sample| sample.0)?;
        let suppressed = merged_suppression(policy.suppressed);
        Ok(SignalBlock {
            samples,
            dependencies,
            suppressed,
            relative_depth,
        })
    }
}

fn merged_suppression<T: Copy + Ord>(mut ranges: Vec<Range<T>>) -> Vec<Range<T>> {
    ranges.sort_unstable_by_key(|range| range.start);
    let mut merged: Vec<Range<T>> = Vec::with_capacity(ranges.len());
    for range in ranges {
        if let Some(last) = merged.last_mut().filter(|last| range.start <= last.end) {
            last.end = last.end.max(range.end);
        } else {
            merged.push(range);
        }
    }
    merged
}

fn validate_timeout(timeout: Duration) -> Result<(), StageAudioError> {
    if timeout.is_zero() || timeout > Duration::from_secs(60) {
        return Err(PreparationError::InvalidRecipe("audio read time budget").into());
    }
    Ok(())
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
    control: WorkControl<'_>,
) -> Result<Vec<Range<SignalSample>>, StageAudioError> {
    let end = SignalSample(
        start
            .0
            .checked_add(i64::try_from(samples.len()).map_err(|_| TimeError::Overflow)?)
            .ok_or(TimeError::Overflow)?,
    );
    let policy = signal.policy(start..end, control.query_limits()?)?;
    control.spend_plan_work(policy.work)?;
    for content in &policy.contents {
        preflight(content, plan)?;
    }
    control.check()?;
    apply_suppression(start, samples, &policy.suppressed, |sample| sample.0)?;
    Ok(merged_suppression(policy.suppressed))
}

fn empty_bound_block(frames: u32) -> SignalBlock {
    SignalBlock {
        samples: vec![[0.0; 2]; frames as usize],
        suppressed: vec![SignalSample(0)..SignalSample(i64::from(frames))],
        ..Default::default()
    }
}

fn apply_suppression<T: Copy>(
    start: T,
    samples: &mut [[f32; 2]],
    ranges: &[Range<T>],
    index: impl Fn(T) -> i64,
) -> Result<(), StageAudioError> {
    let start = index(start);
    let offsets = ranges
        .iter()
        .map(|range| {
            let left = index(range.start)
                .checked_sub(start)
                .and_then(|value| usize::try_from(value).ok())
                .ok_or(StageAudioError::Range)?;
            let right = index(range.end)
                .checked_sub(start)
                .and_then(|value| usize::try_from(value).ok())
                .ok_or(StageAudioError::Range)?;
            if left >= right || right > samples.len() {
                return Err(StageAudioError::Range);
            }
            Ok(left..right)
        })
        .collect::<Result<Vec<_>, _>>()?;
    for range in offsets {
        samples[range].fill([0.0; 2]);
    }
    Ok(())
}

fn root_source_recipe(
    span: &AudioProcessingSpan<'_>,
    rate: u32,
) -> Result<Option<ResampleRecipe>, StageAudioError> {
    let AudioSignalContent::Leaf(AudioContent::Source {
        source, support, ..
    }) = &span.content
    else {
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
        support.start,
        support.end,
    )
}

fn signal_source_recipe(
    span: &AudioSignalSpan<'_>,
    rate: u32,
) -> Result<Option<ResampleRecipe>, StageAudioError> {
    let AudioSignalContent::Leaf(AudioContent::Source {
        source, support, ..
    }) = &span.content
    else {
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
        support.start,
        support.end,
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
    Ok(Some(ResampleRecipe::on_signed_grid(
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

fn root_stage_recipe(
    span: &AudioProcessingSpan<'_>,
    length: usize,
    rate: FrameRate,
) -> Result<ResampleRecipe, StageAudioError> {
    stage_recipe(
        length,
        span.allocated_samples.clone(),
        span.sampling.local_at(span.allocated_samples.start)?,
        span.sampling.local_frames_per_sample(),
        rate,
    )
}

fn signal_stage_recipe(
    span: &AudioSignalSpan<'_>,
    length: usize,
    rate: FrameRate,
) -> Result<ResampleRecipe, StageAudioError> {
    stage_recipe(
        length,
        AudioSample(span.allocated_samples.start.0)..AudioSample(span.allocated_samples.end.0),
        span.sampling.local_at(span.allocated_samples.start)?,
        span.sampling.local_frames_per_sample(),
        rate,
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
    Ok(ResampleRecipe::on_signed_grid(
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

#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod controlled_reads {
    use super::*;
    use deadpan_core::*;
    use deadpan_media::audio_session::{AudioSession, AudioSessionLimits};
    use deadpan_media::source_index::SourceContentIdentity;
    use sha2::{Digest, Sha256};
    use std::io::Cursor;

    pub(super) struct Provider {
        pub(super) prepared: crate::PreparedSource,
        pub(super) calls: usize,
    }

    impl AudioSourceProvider for Provider {
        fn source(
            &mut self,
            _: &ProjectId,
            _: &RevisionId,
            asset: &AssetId,
            _: &AtomicBool,
        ) -> Result<&crate::PreparedSource, PreparationError> {
            assert_eq!(asset, &AssetId::new("media").unwrap());
            self.calls += 1;
            Ok(&self.prepared)
        }
    }

    fn fixture() -> (Arc<RenderPlan>, Provider) {
        fixture_at_rate(FrameRate::new(48_000, 1).unwrap())
    }

    fn fixture_at_rate(rate: FrameRate) -> (Arc<RenderPlan>, Provider) {
        let (document, provider) = fixture_document_at_rate(rate);
        (Arc::new(RenderPlan::compile(&document).unwrap()), provider)
    }

    pub(super) fn fixture_document_at_rate(rate: FrameRate) -> (ProjectDocument, Provider) {
        let bytes = std::fs::read(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../native/deadpan-source/tests/audio-fixtures/pcm-stereo-48000.wav"),
        )
        .unwrap();
        let cancelled = AtomicBool::new(false);
        let session = AudioSession::open_verified(
            &mut Cursor::new(&bytes),
            SourceContentIdentity::new(Sha256::digest(&bytes).into(), bytes.len() as u64).unwrap(),
            0,
            AudioSessionLimits::default(),
            &cancelled,
        )
        .unwrap();
        let index = session.index().clone();
        let prepared = crate::PreparedSource::with_layout(
            session,
            &index,
            AudioChannelLayout::Native {
                channels: 2,
                mask: 3,
            },
            &cancelled,
        )
        .unwrap();
        let id = |value: &str| NodeId::new(value).unwrap();
        let duration = |value| FrameDuration::new(value).unwrap();
        let audio = |start, end| SourceAudio {
            asset: AssetId::new("media").unwrap(),
            span: SourceSpan::new(
                SourceTimestamp {
                    ticks: start,
                    time_base: SourceTimeBase::new(1, 48_000).unwrap(),
                },
                SourceTimestamp {
                    ticks: end,
                    time_base: SourceTimeBase::new(1, 48_000).unwrap(),
                },
            )
            .unwrap(),
        };
        let source = |start, end| BeatNode {
            label: "Source".into(),
            audio_edges: Default::default(),
            kind: NodeKind::Source {
                source: SourceNode {
                    duration: duration(end - start),
                    video: SourceVideo::Blank,
                    video_mapping: SourceVideoMapping::FitBeat,
                    audio: Some(audio(start, end)),
                    audio_mapping: SourceAudioMapping::natural_rate(audio(start, end).span, rate)
                        .unwrap(),
                    audio_offset: AudioSample(0),
                    link: LinkRelation::Independent,
                },
            },
        };
        let mut wire = serde_json::to_value(
            ProjectDocument::new(
                ProjectId::new("controlled-read").unwrap(),
                RevisionId::new("revision").unwrap(),
                PresentationBasis {
                    width: 16,
                    height: 16,
                    frame_rate: rate,
                    color_policy: ColorPolicy::SdrRec709,
                },
                id("root"),
            )
            .unwrap(),
        )
        .unwrap();
        wire["nodes"] = serde_json::to_value(BTreeMap::from([
            (
                id("root"),
                BeatNode::sequence("Root", vec![id("a"), id("room"), id("stage")]),
            ),
            (id("a"), source(0, 512)),
            (
                id("room"),
                BeatNode::hold(
                    "Room",
                    HoldRecipe {
                        duration: duration(128),
                        video: HoldVideo::Background,
                        audio: HoldAudio::RoomTone {
                            source: audio(1024, 1152),
                        },
                    },
                ),
            ),
            (id("b"), source(512, 1024)),
            (
                id("stage"),
                BeatNode {
                    label: "Preserve".into(),
                    audio_edges: Default::default(),
                    kind: NodeKind::Retime {
                        child: id("b"),
                        duration: duration(768),
                        mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(512)).unwrap(),
                        pitch: PitchPolicy::Preserve,
                        purpose: RetimePurpose::Edit,
                    },
                },
            ),
        ]))
        .unwrap();
        wire["assets"] = serde_json::to_value(BTreeMap::from([(
            AssetId::new("media").unwrap(),
            AssetRecord {
                label: "PCM fixture".into(),
                content_hash: "a".repeat(64),
                video: None,
                audio: Some(audio(0, 8197).span),
                still_image: true,
                frame_count: None,
                source_qualification: None,
            },
        )]))
        .unwrap();
        let doc = ProjectDocument::from_json(&wire.to_string()).unwrap();
        (doc, Provider { prepared, calls: 0 })
    }

    #[test]
    fn every_controlled_block_retains_dependencies_already_observed_or_cached() {
        let (plan, mut provider) = fixture();
        let mut renderer = StageAudio::new(plan);
        let work = RefCell::new(ReadWork::default());
        let cancelled = AtomicBool::new(false);
        let control = WorkControl {
            cancelled: &cancelled,
            deadline: Instant::now() + Duration::from_secs(10),
            work: &work,
        };
        let asset = AssetId::new("media").unwrap();
        let fingerprint = control.observe(&asset, &provider.prepared).unwrap();
        let expected = BTreeMap::from([(asset, fingerprint)]);
        for start in [0, 512, 640, 512, 640, 0] {
            let block = renderer
                .read_controlled(&mut provider, AudioSample(start), 64, control, false, 0)
                .unwrap();
            assert_eq!(block.dependencies, expected);
            assert_eq!(block.samples.len(), 64);
        }
        assert_eq!(work.borrow().observed, expected);
        assert_eq!(work.borrow().prepared_stages, 2);
        assert_eq!(renderer.cached_stage_count(), 2);
        assert_eq!(
            provider.calls, 7,
            "each cached dependency is re-admitted, including both source preparation blocks"
        );
    }

    #[test]
    fn controlled_transfer_unions_halo_dependencies_and_keeps_parent_work_limit() {
        let (plan, mut provider) = fixture();
        let domain = plan
            .audio_domain_at(AudioSample(640), Default::default())
            .unwrap();
        let transfer = DomainSignalTransfer::new(
            domain,
            ExactRatio::new(1921, 3).unwrap(),
            SignalSample(0),
            ExactRatio::new(3, 2).unwrap(),
            SignalSample(0)..SignalSample(256),
        )
        .unwrap();
        let mut renderer = StageAudio::new(Arc::clone(&plan));
        let work = RefCell::new(ReadWork::default());
        let cancelled = AtomicBool::new(false);
        let control = WorkControl {
            cancelled: &cancelled,
            deadline: Instant::now() + Duration::from_secs(10),
            work: &work,
        };
        let asset = AssetId::new("media").unwrap();
        let fingerprint = control.observe(&asset, &provider.prepared).unwrap();
        let expected = BTreeMap::from([(asset, fingerprint)]);
        for _ in 0..2 {
            let block = renderer
                .read_domain_transferred_controlled(
                    &mut provider,
                    &transfer,
                    SignalSample(0),
                    256,
                    control,
                    0,
                )
                .unwrap();
            assert_eq!(block.dependencies, expected);
            assert_eq!(block.samples.len(), 256);
        }
        assert_eq!(
            work.borrow().prepared_stages,
            1,
            "halo reads and later reads share preparation"
        );
        assert!(
            provider.calls > 2,
            "interpolation spans several individually admitted halo blocks"
        );

        let mut limited = StageAudio::with_limits(
            Arc::clone(&plan),
            StageLimits {
                maximum_prepared_stages: 1,
                ..Default::default()
            },
        )
        .unwrap();
        let limited_work = RefCell::new(ReadWork::default());
        let limited_control = WorkControl {
            work: &limited_work,
            ..control
        };
        limited
            .read_controlled(
                &mut provider,
                AudioSample(512),
                64,
                limited_control,
                false,
                0,
            )
            .unwrap();
        let calls = provider.calls;
        assert!(matches!(
            limited.read_domain_transferred_controlled(
                &mut provider,
                &transfer,
                SignalSample(0),
                256,
                limited_control,
                0
            ),
            Err(StageAudioError::Limit("prepared stages per read"))
        ));
        assert_eq!(
            provider.calls, calls,
            "the inherited exhausted budget fails before new media work"
        );
        assert_eq!(limited.cached_stage_count(), 1);
    }

    #[test]
    fn placed_point_source_keeps_selected_origin_and_matches_partitioned_pcm() {
        let (plan, mut provider) = fixture();
        let definition = plan
            .audio_definition(AudioDefinitionSelector::Node {
                node: NodeId::new("a").unwrap(),
            })
            .unwrap();
        let domain = definition
            .in_point_clock(
                deadpan_plan::AudioRootPlacement::new(
                    ExactRatio::new(-7, 3).unwrap(),
                    ExactRatio::new(3, 2).unwrap(),
                    ExactRatio::integer(3)..ExactRatio::integer(131),
                )
                .unwrap(),
                ExactRatio::new(5, 7).unwrap(),
            )
            .unwrap();
        assert_eq!(
            domain.reference_samples(),
            ReferenceSample(2)..ReferenceSample(194)
        );
        // Independent source recipe: n=2 lies at source 212/63, with 2/3
        // source sample per selected-grid point and the explicit crop [3,131).
        let recipe = ResampleRecipe::new(
            3..131,
            ExactRatio::new(212, 63).unwrap(),
            AudioSample(0),
            ExactRatio::new(2, 3).unwrap(),
            AudioSample(0)..AudioSample(192),
        )
        .unwrap();
        let cancelled = AtomicBool::new(false);
        let expected = provider
            .prepared
            .prepare(
                recipe,
                AudioSample(0),
                192,
                Duration::from_secs(10),
                &cancelled,
            )
            .unwrap()
            .samples;
        let mut renderer = StageAudio::new(Arc::clone(&plan));
        let full = renderer
            .read_point_domain(
                &mut provider,
                &domain,
                ReferenceSample(2),
                192,
                Duration::from_secs(10),
                &cancelled,
            )
            .unwrap();
        assert_eq!(full.samples, expected);
        assert_eq!(
            full.reference_grid.frame_origin(),
            ExactRatio::new(5, 7).unwrap()
        );
        assert!(full.suppressed.is_empty());
        for (offset, count) in [(127, 65), (0, 51), (51, 76)] {
            let block = renderer
                .read_point_domain(
                    &mut provider,
                    &domain,
                    ReferenceSample(2 + offset),
                    count,
                    Duration::from_secs(10),
                    &cancelled,
                )
                .unwrap();
            assert_eq!(
                block.samples,
                full.samples[offset as usize..offset as usize + count as usize]
            );
        }
        let calls = provider.calls;
        assert!(
            renderer
                .read_point_domain(
                    &mut provider,
                    &domain,
                    ReferenceSample(1),
                    1,
                    Duration::from_secs(10),
                    &cancelled
                )
                .is_err()
        );
        assert!(
            renderer
                .read_point_domain(
                    &mut provider,
                    &domain,
                    ReferenceSample(194),
                    1,
                    Duration::from_secs(10),
                    &cancelled
                )
                .is_err()
        );
        assert_eq!(provider.calls, calls);
    }

    #[test]
    fn placed_ntsc_source_preserves_signed_reference_phase_and_controlled_dependencies() {
        let (plan, mut provider) = fixture_at_rate(FrameRate::new(30_000, 1001).unwrap());
        let definition = plan
            .audio_definition(AudioDefinitionSelector::Node {
                node: NodeId::new("a").unwrap(),
            })
            .unwrap();
        let domain = definition
            .in_point_clock(
                deadpan_plan::AudioRootPlacement::new(
                    ExactRatio::new(-1, 3).unwrap(),
                    ExactRatio::ONE,
                    ExactRatio::ZERO..ExactRatio::new(1, 4).unwrap(),
                )
                .unwrap(),
                ExactRatio::new(1, 7).unwrap(),
            )
            .unwrap();
        assert_eq!(
            domain.reference_samples(),
            ReferenceSample(-762)..ReferenceSample(-362)
        );
        // The source begins at 2/3 of a physical sample. Neither zero-based
        // storage nor the selected 1/7-frame clock origin may reset that phase.
        let recipe = ResampleRecipe::new(
            0..401,
            ExactRatio::new(2, 3).unwrap(),
            AudioSample(0),
            ExactRatio::ONE,
            AudioSample(0)..AudioSample(400),
        )
        .unwrap();
        let cancelled = AtomicBool::new(false);
        let work = RefCell::new(ReadWork::default());
        let control = WorkControl {
            cancelled: &cancelled,
            deadline: Instant::now() + Duration::from_secs(10),
            work: &work,
        };
        let mut renderer = StageAudio::new(Arc::clone(&plan));
        let expected_dependency = BTreeMap::from([(
            AssetId::new("media").unwrap(),
            provider.prepared.provenance(),
        )]);
        for (offset, count) in [(256, 144), (0, 256)] {
            let expected = provider
                .prepared
                .prepare(
                    recipe.clone(),
                    AudioSample(offset),
                    count,
                    Duration::from_secs(10),
                    &cancelled,
                )
                .unwrap()
                .samples;
            let actual = renderer
                .read_point_domain_controlled(
                    &mut provider,
                    &domain,
                    ReferenceSample(-762 + offset),
                    count,
                    control,
                    0,
                )
                .unwrap();
            assert_eq!(actual.samples, expected);
            assert_eq!(actual.dependencies, expected_dependency);
        }
        assert_eq!(work.borrow().observed, expected_dependency);
        let mut foreign = StageAudio::new(fixture().0);
        assert!(matches!(
            foreign.read_point_domain(
                &mut provider,
                &domain,
                ReferenceSample(-762),
                1,
                Duration::from_secs(10),
                &cancelled
            ),
            Err(StageAudioError::ForeignDomain)
        ));
    }

    #[test]
    fn placed_point_preserve_rechecks_policy_that_owned_no_input_point() {
        let (_, mut provider) = fixture();
        let cancelled = AtomicBool::new(false);
        let id = |value: &str| NodeId::new(value).unwrap();
        let duration = |value| FrameDuration::new(value).unwrap();
        let rate = FrameRate::new(192_000, 1).unwrap();
        let audio = |start, end| SourceAudio {
            asset: AssetId::new("media").unwrap(),
            span: SourceSpan::new(
                SourceTimestamp {
                    ticks: start,
                    time_base: SourceTimeBase::new(1, 48_000).unwrap(),
                },
                SourceTimestamp {
                    ticks: end,
                    time_base: SourceTimeBase::new(1, 48_000).unwrap(),
                },
            )
            .unwrap(),
        };
        let source = |at| BeatNode {
            label: "Source".into(),
            audio_edges: Default::default(),
            kind: NodeKind::Source {
                source: SourceNode {
                    duration: duration(1),
                    video: SourceVideo::Blank,
                    video_mapping: SourceVideoMapping::FitBeat,
                    audio: Some(audio(at, at + 1)),
                    audio_mapping: SourceAudioMapping::natural_rate(audio(at, at + 1).span, rate)
                        .unwrap(),
                    audio_offset: AudioSample(0),
                    link: LinkRelation::Independent,
                },
            },
        };
        let retime = |child: &str, output, selected| BeatNode {
            label: "Preserve".into(),
            audio_edges: Default::default(),
            kind: NodeKind::Retime {
                child: id(child),
                duration: duration(output),
                mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(selected)).unwrap(),
                pitch: PitchPolicy::Preserve,
                purpose: RetimePurpose::Edit,
            },
        };
        let mut wire = serde_json::to_value(
            ProjectDocument::new(
                ProjectId::new("point-policy").unwrap(),
                RevisionId::new("r0").unwrap(),
                PresentationBasis {
                    width: 16,
                    height: 16,
                    frame_rate: rate,
                    color_policy: ColorPolicy::SdrRec709,
                },
                id("root"),
            )
            .unwrap(),
        )
        .unwrap();
        wire["nodes"] = serde_json::to_value(BTreeMap::from([
            (id("root"), BeatNode::sequence("Root", vec![id("outer")])),
            (id("a"), source(0)),
            (
                id("silent"),
                BeatNode::hold(
                    "No input point",
                    HoldRecipe {
                        duration: duration(1),
                        video: HoldVideo::Background,
                        audio: HoldAudio::Silence,
                    },
                ),
            ),
            (id("b"), source(1)),
            (
                id("cuts"),
                BeatNode::sequence("Cuts", vec![id("a"), id("silent"), id("b")]),
            ),
            (id("inner"), retime("cuts", 24, 3)),
            (id("outer"), retime("inner", 48, 24)),
        ]))
        .unwrap();
        wire["assets"] = serde_json::to_value(BTreeMap::from([(
            AssetId::new("media").unwrap(),
            AssetRecord {
                label: "PCM".into(),
                content_hash: "a".repeat(64),
                video: None,
                audio: Some(audio(0, 8197).span),
                still_image: true,
                frame_count: None,
                source_qualification: None,
            },
        )]))
        .unwrap();
        let plan = Arc::new(
            RenderPlan::compile(&ProjectDocument::from_json(&wire.to_string()).unwrap()).unwrap(),
        );
        let definition = plan
            .audio_definition(AudioDefinitionSelector::Node { node: id("outer") })
            .unwrap();
        let domain = definition
            .in_point_clock(
                deadpan_plan::AudioRootPlacement::new(
                    ExactRatio::new(-1, 8).unwrap(),
                    ExactRatio::ONE,
                    ExactRatio::ZERO..ExactRatio::integer(48),
                )
                .unwrap(),
                ExactRatio::new(131, 8).unwrap(),
            )
            .unwrap();
        assert_eq!(
            domain.reference_samples(),
            ReferenceSample(-4)..ReferenceSample(8)
        );
        // Source, Hold, Source each occupy 1/4 of the inner input grid. Only
        // Source A owns an input point. The Hold must nevertheless suppress
        // inner output [2,4), outer [4,8), and final reference labels [0,4).
        let stretch = |input: &[[f32; 2]], output, numerator, denominator| {
            let input = StereoPcm::new(
                input.iter().map(|x| x[0]).collect(),
                input.iter().map(|x| x[1]).collect(),
            )
            .unwrap();
            let recipe = CanonicalRecipe::with_rate(
                input.frames(),
                output,
                StretchRate::new(numerator, denominator).unwrap(),
                0,
            )
            .unwrap();
            let mut dsp = CanonicalStretch::new(recipe, input).unwrap();
            let mut left = vec![0.0; output as usize];
            let mut right = vec![0.0; output as usize];
            assert_eq!(
                dsp.read(&mut left, &mut right, &cancelled).unwrap(),
                output as usize
            );
            left.into_iter()
                .zip(right)
                .map(|(l, r)| [l, r])
                .collect::<Vec<_>>()
        };
        let mut inner = stretch(&[[0.75, -1.0]], 6, 1, 8);
        inner[2..4].fill([0.0; 2]);
        let mut outer = stretch(&inner, 12, 1, 2);
        outer[4..8].fill([0.0; 2]);
        let mut expected = sample_prepared(
            &outer,
            ResampleRecipe::new(
                0..12,
                ExactRatio::new(1, 8).unwrap(),
                AudioSample(0),
                ExactRatio::ONE,
                AudioSample(0)..AudioSample(12),
            )
            .unwrap(),
            AudioSample(0),
            12,
            &cancelled,
        )
        .unwrap();
        expected[4..8].fill([0.0; 2]);
        let mut renderer = StageAudio::new(Arc::clone(&plan));
        let full = renderer
            .read_point_domain(
                &mut provider,
                &domain,
                ReferenceSample(-4),
                12,
                Duration::from_secs(10),
                &cancelled,
            )
            .unwrap();
        assert_eq!(full.samples, expected);
        assert_eq!(
            full.suppressed,
            vec![ReferenceSample(0)..ReferenceSample(4)]
        );
        assert!(
            full.samples[..4]
                .iter()
                .chain(&full.samples[8..])
                .flatten()
                .any(|sample| sample.abs() > 1e-5)
        );
        for (start, count) in [(4, 4), (-4, 3), (-1, 5)] {
            let block = renderer
                .read_point_domain(
                    &mut provider,
                    &domain,
                    ReferenceSample(start),
                    count,
                    Duration::from_secs(10),
                    &cancelled,
                )
                .unwrap();
            assert_eq!(
                block.samples,
                full.samples[(start + 4) as usize..(start + 4) as usize + count as usize]
            );
        }
        assert_eq!(renderer.cached_stage_count(), 2);
    }

    #[test]
    fn controlled_depth_cannot_restart_at_a_domain_or_bypass_through_cache() {
        let (plan, mut provider) = fixture();
        let mut renderer = StageAudio::with_limits(
            Arc::clone(&plan),
            StageLimits {
                maximum_depth: 1,
                ..Default::default()
            },
        )
        .unwrap();
        let cancelled = AtomicBool::new(false);
        let work = RefCell::new(ReadWork::default());
        let control = WorkControl {
            cancelled: &cancelled,
            deadline: Instant::now() + Duration::from_secs(10),
            work: &work,
        };
        for start in [512, 640] {
            let domain = plan
                .audio_domain_at(AudioSample(start), Default::default())
                .unwrap();
            renderer
                .read_domain_controlled(&mut provider, &domain, AudioSample(start), 64, control, 0)
                .unwrap();
            let calls = provider.calls;
            assert!(matches!(
                renderer.read_domain_controlled(
                    &mut provider,
                    &domain,
                    AudioSample(start),
                    64,
                    control,
                    1
                ),
                Err(StageAudioError::Limit("nested stage depth"))
            ));
            assert_eq!(
                provider.calls, calls,
                "depth rejection precedes cache admission"
            );
        }
        assert_eq!(renderer.cached_stage_count(), 2);
    }
}

#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
#[path = "bound_reads.rs"]
mod bound_reads;

#[cfg(test)]
mod sampling_recipes {
    use super::*;
    use crate::sequence::sampling_recipes::{
        assert_resumed_pcm, fixture_plan, generated_pcm, ratio, render,
    };
    use deadpan_plan::AudioSampleMap;

    #[test]
    fn processing_source_recipes_use_resumed_root_and_signal_maps() {
        let plan = fixture_plan(false);
        let mut root = plan
            .audio_processing(AudioSample(0)..AudioSample(1), Default::default())
            .unwrap()
            .spans
            .remove(0);
        let mut signal = plan
            .audio_signal()
            .query(SignalSample(0)..SignalSample(1), Default::default())
            .unwrap()
            .spans
            .remove(0);
        let root_transform = root.transform;
        let signal_transform = signal.transform;
        let root_recipe = root_source_recipe(&root, 44_100).unwrap().unwrap();
        let signal_recipe = signal_source_recipe(&signal, 44_100).unwrap().unwrap();
        assert_eq!(root_recipe.source_step(), ratio(147, 160));
        assert_eq!(signal_recipe.source_step(), ratio(147, 160));
        let input = generated_pcm(44_100);
        // Root round-even and signal ceil grids have different insertion sizes.
        for (cut, root_anchor, signal_anchor, root_old, signal_old) in [
            (1602, 3203, 3204, 1602, 1602),
            (4805, 6406, 6407, 3204, 3203),
        ] {
            root.sampling = root
                .sampling
                .resume(AudioSample(cut), AudioSample(root_anchor))
                .unwrap();
            root.allocated_samples.start = AudioSample(root_anchor);
            root.samples = AudioSample(root_anchor)..AudioSample(root_anchor + 256);
            signal.sampling = signal
                .sampling
                .resume(SignalSample(cut), SignalSample(signal_anchor))
                .unwrap();
            signal.allocated_samples.start = SignalSample(signal_anchor);
            signal.samples = SignalSample(signal_anchor)..SignalSample(signal_anchor + 256);
            assert_eq!(root.transform, root_transform);
            assert_eq!(signal.transform, signal_transform);
            let resumed_root = root_source_recipe(&root, 44_100).unwrap().unwrap();
            let resumed_signal = signal_source_recipe(&signal, 44_100).unwrap().unwrap();
            assert_resumed_pcm(&input, &root_recipe, &resumed_root, root_old, root_anchor);
            assert_resumed_pcm(
                &input,
                &signal_recipe,
                &resumed_signal,
                signal_old,
                signal_anchor,
            );
            root.samples.start = AudioSample(root_anchor + 173);
            signal.samples.start = SignalSample(signal_anchor + 173);
            assert_eq!(
                root_source_recipe(&root, 44_100).unwrap().unwrap(),
                resumed_root
            );
            assert_eq!(
                signal_source_recipe(&signal, 44_100).unwrap().unwrap(),
                resumed_signal
            );
        }
    }

    fn prepared_pcm(stage: &AudioStage<'_>) -> Vec<[f32; 2]> {
        let input = generated_pcm(stage.input_signal().sample_count().unwrap().0 as usize);
        let output_count = stage.output_signal().sample_count().unwrap().0 as u32;
        let rate = stage.descriptor().rate;
        let recipe = CanonicalRecipe::with_rate(
            input.len() as u32,
            output_count,
            StretchRate::new(rate.numerator() as u64, rate.denominator() as u64).unwrap(),
            0,
        )
        .unwrap();
        let pcm = StereoPcm::new(
            input.iter().map(|sample| sample[0]).collect(),
            input.iter().map(|sample| sample[1]).collect(),
        )
        .unwrap();
        let mut dsp = CanonicalStretch::new(recipe, pcm).unwrap();
        let mut output = Vec::new();
        while output.len() < output_count as usize {
            let count = (output_count as usize - output.len()).min(256);
            let mut left = vec![0.0; count];
            let mut right = vec![0.0; count];
            assert_eq!(
                dsp.read(&mut left, &mut right, &AtomicBool::new(false))
                    .unwrap(),
                count
            );
            output.extend(
                left.into_iter()
                    .zip(right)
                    .map(|(left, right)| [left, right]),
            );
        }
        output
    }

    #[test]
    fn prepared_stage_recipes_resume_full_canonical_pcm_without_repreparation() {
        let plan = fixture_plan(true);
        let rate = plan.metadata().presentation_basis.frame_rate;
        let mut root = plan
            .audio_processing(AudioSample(0)..AudioSample(1), Default::default())
            .unwrap()
            .spans
            .remove(0);
        let mut signal = plan
            .audio_signal()
            .query(SignalSample(0)..SignalSample(1), Default::default())
            .unwrap()
            .spans
            .remove(0);
        let AudioSignalContent::Stage(stage) = &root.content else {
            panic!("expected Preserve stage")
        };
        let descriptor = stage.descriptor().clone();
        let input_count = stage.input_signal().sample_count().unwrap();
        let input_query = serde_json::to_value(
            stage
                .input_signal()
                .query(SignalSample(0)..SignalSample(256), Default::default())
                .unwrap(),
        )
        .unwrap();
        let prepared = prepared_pcm(stage);
        let root_transform = root.transform;
        let signal_transform = signal.transform;
        let original_root = root_stage_recipe(&root, prepared.len(), rate).unwrap();
        let original_signal = signal_stage_recipe(&signal, prepared.len(), rate).unwrap();
        assert_eq!(original_root.selection(), 0..96_096);
        assert_eq!(original_root.source_step(), ExactRatio::ONE);
        for (cut, root_anchor, signal_anchor, root_old, signal_old) in [
            (1602, 3203, 3204, 1602, 1602),
            (4805, 6406, 6407, 3204, 3203),
        ] {
            root.sampling = root
                .sampling
                .resume(AudioSample(cut), AudioSample(root_anchor))
                .unwrap();
            root.allocated_samples.start = AudioSample(root_anchor);
            root.samples = AudioSample(root_anchor)..AudioSample(root_anchor + 256);
            signal.sampling = signal
                .sampling
                .resume(SignalSample(cut), SignalSample(signal_anchor))
                .unwrap();
            signal.allocated_samples.start = SignalSample(signal_anchor);
            signal.samples = SignalSample(signal_anchor)..SignalSample(signal_anchor + 256);
            assert_eq!(root.transform, root_transform);
            assert_eq!(signal.transform, signal_transform);
            for (recipe, original, old, anchor) in [
                (
                    root_stage_recipe(&root, prepared.len(), rate).unwrap(),
                    &original_root,
                    root_old,
                    root_anchor,
                ),
                (
                    signal_stage_recipe(&signal, prepared.len(), rate).unwrap(),
                    &original_signal,
                    signal_old,
                    signal_anchor,
                ),
            ] {
                assert_resumed_pcm(&prepared, original, &recipe, old, anchor);
                // The actual prepared-buffer consumer must share this phase,
                // including a suffix read before any earlier output is requested.
                assert_eq!(
                    sample_prepared(
                        &prepared,
                        recipe.clone(),
                        AudioSample(anchor + 173),
                        83,
                        &AtomicBool::new(false)
                    )
                    .unwrap(),
                    render(&prepared, original, old + 173, 83)
                );
                assert_eq!(
                    sample_prepared(
                        &prepared,
                        recipe,
                        AudioSample(anchor),
                        256,
                        &AtomicBool::new(false)
                    )
                    .unwrap(),
                    render(&prepared, original, old, 256)
                );
            }
        }
        let AudioSignalContent::Stage(retained) = &root.content else {
            unreachable!()
        };
        assert_eq!(retained.descriptor(), &descriptor);
        assert_eq!(retained.input_signal().sample_count().unwrap(), input_count);
        assert_eq!(
            serde_json::to_value(
                retained
                    .input_signal()
                    .query(SignalSample(0)..SignalSample(256), Default::default())
                    .unwrap()
            )
            .unwrap(),
            input_query
        );
    }

    #[test]
    fn prepared_and_source_recipe_steps_come_from_sampling_not_structure() {
        let plan = fixture_plan(false);
        let rate = plan.metadata().presentation_basis.frame_rate;
        let mut root = plan
            .audio_processing(AudioSample(0)..AudioSample(1), Default::default())
            .unwrap()
            .spans
            .remove(0);
        let mut signal = plan
            .audio_signal()
            .query(SignalSample(0)..SignalSample(1), Default::default())
            .unwrap()
            .spans
            .remove(0);
        let step = ratio(15, 16016); // 3/2 prepared samples per output sample.
        root.sampling = AudioSampleMap::new(AudioSample(0), ExactRatio::ZERO, step)
            .unwrap()
            .resume(AudioSample(1602), AudioSample(3203))
            .unwrap();
        signal.sampling = AudioSampleMap::new(SignalSample(0), ExactRatio::ZERO, step)
            .unwrap()
            .resume(SignalSample(1602), SignalSample(3204))
            .unwrap();
        root.allocated_samples.start = AudioSample(3203);
        signal.allocated_samples.start = SignalSample(3204);
        assert_eq!(
            root_source_recipe(&root, 44_100)
                .unwrap()
                .unwrap()
                .source_step(),
            ratio(441, 320)
        );
        assert_eq!(
            signal_source_recipe(&signal, 44_100)
                .unwrap()
                .unwrap()
                .source_step(),
            ratio(441, 320)
        );
        let input = generated_pcm(48_000);
        for (recipe, anchor) in [
            (root_stage_recipe(&root, input.len(), rate).unwrap(), 3203),
            (
                signal_stage_recipe(&signal, input.len(), rate).unwrap(),
                3204,
            ),
        ] {
            assert_eq!(recipe.source_origin(), ExactRatio::integer(2403));
            assert_eq!(recipe.source_step(), ratio(3, 2));
            assert_eq!(recipe.selection(), 0..48_000);
            let expected = ResampleRecipe::new(
                0..48_000,
                ExactRatio::integer(2403),
                AudioSample(anchor),
                ratio(3, 2),
                recipe.output_range(),
            )
            .unwrap();
            assert_eq!(
                sample_prepared(
                    &input,
                    recipe,
                    AudioSample(anchor),
                    256,
                    &AtomicBool::new(false)
                )
                .unwrap(),
                render(&input, &expected, anchor, 256)
            );
        }
    }
}
