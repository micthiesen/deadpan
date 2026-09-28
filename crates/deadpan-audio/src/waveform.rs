//! Bounded signed stereo extrema on an exact definition-output sample grid.
//! Unknown coverage is never represented by zero-valued measured bins.

use std::fmt;
use std::mem::size_of;
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use deadpan_core::{ExactRatio, FrameDuration, FrameRate, NodeId, ProjectId, RevisionId};
use deadpan_plan::{AudioDefinitionSelector, AudioSampleGrid, SignalSample};

pub const MAX_WAVEFORM_BYTES: usize = 1024 * 1024;
pub const MAX_WAVEFORM_LEAVES: u32 = 4096;
pub const MAX_WAVEFORM_SAMPLES: u64 = 120 * 48_000;
pub const WAVEFORM_STAGE: &str = "definition_output_pcm_before_effects";
const MIN_LEAF_STRIDE: u64 = 256;
const MAX_LEVELS: usize = 13;
const MAX_TIMEOUT: Duration = Duration::from_secs(20);
const ARC_HEADER_BYTES: usize = 2 * size_of::<usize>();

#[derive(Debug, thiserror::Error)]
pub enum WaveformError {
    #[error("waveform limits exceed the bounded overview contract")]
    InvalidLimits,
    #[error("waveform memory limit must be within 1..=1048576 bytes")]
    InvalidMemoryLimit,
    #[error("waveform peak allocations exceed their aggregate byte budget")]
    MemoryLimit,
    #[error("waveform peak allocation failed")]
    Allocation,
    #[error("waveform blocks require 1..=256 finite stereo samples")]
    InvalidSamples,
    #[error("waveform input must be contiguous and inside its declared extent")]
    InvalidSequence,
    #[error("waveform geometry or bin index is invalid")]
    InvalidGeometry,
    #[error("waveform geometry or byte accounting overflowed")]
    Overflow,
    #[error(transparent)]
    Time(#[from] deadpan_core::TimeError),
    #[error(transparent)]
    Plan(#[from] deadpan_plan::PlanError),
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[error(transparent)]
    Stage(#[from] crate::StageAudioError),
}

#[derive(Debug, Clone, Copy)]
pub struct WaveformLimits {
    maximum_samples: u64,
    maximum_leaves: u32,
    timeout: Duration,
}

impl WaveformLimits {
    pub fn new(
        maximum_samples: u64,
        maximum_leaves: u32,
        timeout: Duration,
    ) -> Result<Self, WaveformError> {
        if maximum_samples == 0
            || maximum_samples > MAX_WAVEFORM_SAMPLES
            || maximum_leaves == 0
            || maximum_leaves > MAX_WAVEFORM_LEAVES
            || timeout.is_zero()
            || timeout > MAX_TIMEOUT
        {
            return Err(WaveformError::InvalidLimits);
        }
        Ok(Self {
            maximum_samples,
            maximum_leaves,
            timeout,
        })
    }

    pub fn maximum_samples(self) -> u64 {
        self.maximum_samples
    }
    pub fn maximum_leaves(self) -> u32 {
        self.maximum_leaves
    }
    pub fn timeout(self) -> Duration {
        self.timeout
    }
}

impl Default for WaveformLimits {
    fn default() -> Self {
        Self {
            maximum_samples: MAX_WAVEFORM_SAMPLES,
            maximum_leaves: MAX_WAVEFORM_LEAVES,
            timeout: MAX_TIMEOUT,
        }
    }
}

#[derive(Debug)]
struct MemoryState {
    maximum: usize,
    resident: AtomicUsize,
}

/// One ledger is shared by the worker, mailbox and UI. Cloning a result does
/// not release its allocation charge; its final Arc drop does. This measures
/// peak storage/metadata, not source caches, native DSP or total process RSS.
#[derive(Debug, Clone)]
pub struct WaveformMemory {
    state: Arc<MemoryState>,
}

impl WaveformMemory {
    pub fn new(maximum_bytes: usize) -> Result<Self, WaveformError> {
        if maximum_bytes == 0 || maximum_bytes > MAX_WAVEFORM_BYTES {
            return Err(WaveformError::InvalidMemoryLimit);
        }
        Ok(Self {
            state: Arc::new(MemoryState {
                maximum: maximum_bytes,
                resident: AtomicUsize::new(0),
            }),
        })
    }

    pub fn maximum_bytes(&self) -> usize {
        self.state.maximum
    }
    pub fn resident_bytes(&self) -> usize {
        self.state.resident.load(Ordering::Acquire)
    }

    fn reserve(&self, bytes: usize) -> Result<MemoryPermit, WaveformError> {
        let mut current = self.resident_bytes();
        loop {
            let next = current
                .checked_add(bytes)
                .filter(|next| *next <= self.state.maximum)
                .ok_or(WaveformError::MemoryLimit)?;
            match self.state.resident.compare_exchange_weak(
                current,
                next,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    return Ok(MemoryPermit {
                        state: Arc::clone(&self.state),
                        bytes,
                    });
                }
                Err(observed) => current = observed,
            }
        }
    }
}

impl Default for WaveformMemory {
    fn default() -> Self {
        Self {
            state: Arc::new(MemoryState {
                maximum: MAX_WAVEFORM_BYTES,
                resident: AtomicUsize::new(0),
            }),
        }
    }
}

#[derive(Debug)]
struct MemoryPermit {
    state: Arc<MemoryState>,
    bytes: usize,
}

impl Drop for MemoryPermit {
    fn drop(&mut self) {
        self.state.resident.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}

pub struct WaveformControl<'a> {
    pub limits: WaveformLimits,
    pub cancelled: &'a AtomicBool,
    pub memory: &'a WaveformMemory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WaveformStopReason {
    OutputLimit,
    Cancelled,
    Deadline,
    Preparation(String),
}

impl fmt::Display for WaveformStopReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutputLimit => f.write_str("overview output-sample limit reached"),
            Self::Cancelled => f.write_str("overview measurement interrupted"),
            Self::Deadline => f.write_str("overview preparation deadline expired"),
            Self::Preparation(error) => f.write_str(error),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WaveformCompletion {
    Complete,
    Partial(WaveformStopReason),
}

#[derive(Debug)]
pub struct WaveformMeasurement {
    pub waveform: Arc<DefinitionWaveform>,
    pub completion: WaveformCompletion,
    /// Successfully reduced samples, including any unpublished partial leaf.
    pub examined_samples: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaveformDescriptor {
    pub stage: &'static str,
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub definition: AudioDefinitionSelector,
    pub root: NodeId,
    pub frame_rate: FrameRate,
    pub owner_duration: FrameDuration,
    pub sample_rate: u32,
    pub grid: AudioSampleGrid<SignalSample>,
    pub total_samples: SignalSample,
    pub leaf_stride: u64,
}

#[derive(Debug)]
struct DescriptorAllocation {
    value: WaveformDescriptor,
    _permit: MemoryPermit,
}

/// Signed extrema for the left and right channels. Neither channel is rectified
/// or normalized. Finite over-unity samples remain present without clipping.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StereoExtrema {
    minimum: [f32; 2],
    maximum: [f32; 2],
}

impl StereoExtrema {
    pub fn minimum(self) -> [f32; 2] {
        self.minimum
    }
    pub fn maximum(self) -> [f32; 2] {
        self.maximum
    }

    fn sample(sample: [f32; 2]) -> Self {
        Self {
            minimum: sample,
            maximum: sample,
        }
    }

    fn merge(self, other: Self) -> Self {
        let minimum = std::array::from_fn(|channel| {
            if self.minimum[channel]
                .total_cmp(&other.minimum[channel])
                .is_le()
            {
                self.minimum[channel]
            } else {
                other.minimum[channel]
            }
        });
        let maximum = std::array::from_fn(|channel| {
            if self.maximum[channel]
                .total_cmp(&other.maximum[channel])
                .is_ge()
            {
                self.maximum[channel]
            } else {
                other.maximum[channel]
            }
        });
        Self { minimum, maximum }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Level {
    offset: usize,
    capacity: usize,
    measured: usize,
    stride: u64,
}

/// Immutable, media-free display data for one admitted request. This is not a
/// source-admission witness or an arbitrary cropped-range peak query.
#[derive(Debug)]
pub struct DefinitionWaveform {
    descriptor: Arc<DescriptorAllocation>,
    levels: [Level; MAX_LEVELS],
    level_count: usize,
    measured_end: SignalSample,
    peaks: Vec<StereoExtrema>,
    _permit: MemoryPermit,
}

impl DefinitionWaveform {
    pub fn descriptor(&self) -> &WaveformDescriptor {
        &self.descriptor.value
    }
    pub fn measured_end(&self) -> SignalSample {
        self.measured_end
    }
    pub fn level_count(&self) -> usize {
        self.level_count
    }

    /// Coarser levels contain only bins whose complete support was measured.
    /// An unpaired prefix leaf is not promoted as a complete parent before EOF.
    pub fn level(&self, level: usize) -> Option<&[StereoExtrema]> {
        let level = self
            .levels
            .get(level)
            .filter(|_| level < self.level_count)?;
        self.peaks
            .get(level.offset..level.offset.checked_add(level.measured)?)
    }

    pub fn bin_samples(&self, level: usize, index: usize) -> Option<Range<SignalSample>> {
        let level = self
            .levels
            .get(level)
            .filter(|_| level < self.level_count)?;
        if index >= level.measured {
            return None;
        }
        let start = u64::try_from(index).ok()?.checked_mul(level.stride)?;
        let end = start
            .checked_add(level.stride)?
            .min(u64::try_from(self.descriptor().total_samples.0).ok()?);
        Some(SignalSample(i64::try_from(start).ok()?)..SignalSample(i64::try_from(end).ok()?))
    }

    /// PointCeil's terminal sample boundary can lie past the exact owner end.
    /// Clip only that display endpoint, never rescale the preceding sample grid.
    pub fn bin_owner_frames(
        &self,
        level: usize,
        index: usize,
    ) -> Result<Range<ExactRatio>, WaveformError> {
        let range = self
            .bin_samples(level, index)
            .ok_or(WaveformError::InvalidGeometry)?;
        let descriptor = self.descriptor();
        let start = descriptor.grid.at(range.start)?;
        let end = if range.end == descriptor.total_samples {
            ExactRatio::integer(descriptor.owner_duration.frames())
        } else {
            descriptor.grid.at(range.end)?
        };
        Ok(start..end)
    }

    fn allocate(
        descriptor: Arc<DescriptorAllocation>,
        memory: &WaveformMemory,
    ) -> Result<Self, WaveformError> {
        let count = u64::try_from(descriptor.value.total_samples.0)
            .map_err(|_| WaveformError::InvalidGeometry)?;
        let mut remaining = count.div_ceil(descriptor.value.leaf_stride);
        let mut stride = descriptor.value.leaf_stride;
        let mut levels = [Level::default(); MAX_LEVELS];
        let mut level_count = 0;
        let mut slots = 0_usize;
        while remaining > 0 {
            let capacity = usize::try_from(remaining).map_err(|_| WaveformError::Overflow)?;
            let level = levels
                .get_mut(level_count)
                .ok_or(WaveformError::InvalidGeometry)?;
            *level = Level {
                offset: slots,
                capacity,
                measured: 0,
                stride,
            };
            slots = slots.checked_add(capacity).ok_or(WaveformError::Overflow)?;
            level_count += 1;
            if remaining == 1 {
                break;
            }
            remaining = remaining.div_ceil(2);
            stride = stride.checked_mul(2).ok_or(WaveformError::Overflow)?;
        }
        let bytes = slots
            .checked_mul(size_of::<StereoExtrema>())
            .and_then(|bytes| bytes.checked_add(size_of::<Self>() + ARC_HEADER_BYTES))
            .ok_or(WaveformError::Overflow)?;
        let mut permit = memory.reserve(bytes)?;
        let mut peaks = Vec::new();
        peaks
            .try_reserve_exact(slots)
            .map_err(|_| WaveformError::Allocation)?;
        // Charge the Vec's actual capacity, including any allocator over-allocation.
        if peaks.capacity() > slots {
            let extra = (peaks.capacity() - slots)
                .checked_mul(size_of::<StereoExtrema>())
                .ok_or(WaveformError::Overflow)?;
            let mut extra_permit = memory.reserve(extra)?;
            permit.bytes = permit
                .bytes
                .checked_add(extra)
                .ok_or(WaveformError::Overflow)?;
            extra_permit.bytes = 0;
        }
        peaks.resize(slots, StereoExtrema::sample([0.0; 2]));
        Ok(Self {
            descriptor,
            levels,
            level_count,
            measured_end: SignalSample(0),
            peaks,
            _permit: permit,
        })
    }
}

/// Pure accumulator. It owns one final-result allocation from the start, so a
/// retained progress snapshot cannot prevent terminal publication of its prefix.
pub(crate) struct WaveformBuilder {
    data: DefinitionWaveform,
    examined: u64,
    pending: Option<StereoExtrema>,
    pending_samples: u64,
}

impl WaveformBuilder {
    pub(crate) fn new(
        mut descriptor: WaveformDescriptor,
        limits: WaveformLimits,
        memory: &WaveformMemory,
    ) -> Result<Self, WaveformError> {
        let total = u64::try_from(descriptor.total_samples.0)
            .map_err(|_| WaveformError::InvalidGeometry)?;
        if descriptor.sample_rate != deadpan_core::MIX_SAMPLE_RATE
            || descriptor.grid.boundary_rule() != deadpan_plan::AudioBoundaryRule::PointCeil
            || descriptor.grid.frame_origin() != ExactRatio::ZERO
            || descriptor
                .grid
                .boundary(ExactRatio::integer(descriptor.owner_duration.frames()))?
                != descriptor.total_samples
        {
            return Err(WaveformError::InvalidGeometry);
        }
        let mut stride = MIN_LEAF_STRIDE;
        while total.div_ceil(stride) > u64::from(limits.maximum_leaves) {
            stride = stride.checked_mul(2).ok_or(WaveformError::Overflow)?;
        }
        descriptor.leaf_stride = stride;
        // Four identifier strings have validated lengths <= MAX_IDENTITY_BYTES.
        // Charge their complete bounded capacity in addition to the Arc payload.
        let bytes = size_of::<DescriptorAllocation>()
            + ARC_HEADER_BYTES
            + 4 * deadpan_core::MAX_IDENTITY_BYTES;
        let permit = memory.reserve(bytes)?;
        let descriptor = Arc::new(DescriptorAllocation {
            value: descriptor,
            _permit: permit,
        });
        let data = DefinitionWaveform::allocate(descriptor, memory)?;
        Ok(Self {
            data,
            examined: 0,
            pending: None,
            pending_samples: 0,
        })
    }

    pub(crate) fn examined_samples(&self) -> u64 {
        self.examined
    }
    pub(crate) fn measured_end(&self) -> SignalSample {
        self.data.measured_end
    }

    /// Validate the complete block before advancing either hidden partial state
    /// or published coverage. No failed block can leave partly admitted extrema.
    pub(crate) fn push(
        &mut self,
        start: SignalSample,
        samples: &[[f32; 2]],
    ) -> Result<(), WaveformError> {
        let frame_count =
            u32::try_from(samples.len()).map_err(|_| WaveformError::InvalidSamples)?;
        if frame_count == 0
            || frame_count > crate::MAX_OUTPUT_FRAMES
            || samples.iter().flatten().any(|sample| !sample.is_finite())
        {
            return Err(WaveformError::InvalidSamples);
        }
        let count = u64::from(frame_count);
        let end = self
            .examined
            .checked_add(count)
            .ok_or(WaveformError::Overflow)?;
        let total = u64::try_from(self.data.descriptor().total_samples.0)
            .map_err(|_| WaveformError::InvalidGeometry)?;
        if u64::try_from(start.0).ok() != Some(self.examined) || end > total {
            return Err(WaveformError::InvalidSequence);
        }
        for &sample in samples {
            let next = StereoExtrema::sample(sample);
            self.pending = Some(self.pending.map_or(next, |prior| prior.merge(next)));
            self.pending_samples += 1;
            self.examined += 1;
            if self.pending_samples == self.data.descriptor().leaf_stride || self.examined == total
            {
                self.complete_leaf(self.examined == total)?;
            }
        }
        Ok(())
    }

    fn complete_leaf(&mut self, eof: bool) -> Result<(), WaveformError> {
        let value = self.pending.take().ok_or(WaveformError::InvalidGeometry)?;
        self.pending_samples = 0;
        let leaf = &mut self.data.levels[0];
        if leaf.measured >= leaf.capacity {
            return Err(WaveformError::InvalidGeometry);
        }
        self.data.peaks[leaf.offset + leaf.measured] = value;
        leaf.measured += 1;
        self.data.measured_end =
            SignalSample(i64::try_from(self.examined).map_err(|_| WaveformError::Overflow)?);
        for index in 1..self.data.level_count {
            let child = self.data.levels[index - 1];
            if !child.measured.is_multiple_of(2) && !eof {
                break;
            }
            let parent = &mut self.data.levels[index];
            let start = parent.measured * 2;
            if start >= child.measured {
                break;
            }
            let mut value = self.data.peaks[child.offset + start];
            if start + 1 < child.measured {
                value = value.merge(self.data.peaks[child.offset + start + 1]);
            }
            self.data.peaks[parent.offset + parent.measured] = value;
            parent.measured += 1;
        }
        Ok(())
    }

    pub(crate) fn snapshot(
        &self,
        memory: &WaveformMemory,
    ) -> Result<Arc<DefinitionWaveform>, WaveformError> {
        let mut copy = DefinitionWaveform::allocate(Arc::clone(&self.data.descriptor), memory)?;
        copy.measured_end = self.data.measured_end;
        for index in 0..self.data.level_count {
            let level = self.data.levels[index];
            copy.levels[index].measured = level.measured;
            let range = level.offset..level.offset + level.measured;
            copy.peaks[range.clone()].copy_from_slice(&self.data.peaks[range]);
        }
        Ok(Arc::new(copy))
    }

    pub(crate) fn finish(self, completion: WaveformCompletion) -> WaveformMeasurement {
        WaveformMeasurement {
            waveform: Arc::new(self.data),
            completion,
            examined_samples: self.examined,
        }
    }
}

#[cfg(test)]
#[path = "waveform_tests.rs"]
mod tests;
