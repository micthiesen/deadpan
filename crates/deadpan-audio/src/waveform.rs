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

mod peaks;
use peaks::{PeakAccumulator, PeakBuffer};

mod edit_window;
pub(crate) use edit_window::EditWaveformBuilder;
pub use edit_window::{
    EditWaveform, EditWaveformDescriptor, EditWaveformMeasurement, EditWaveformStage,
};

/// Immutable, media-free display data for one admitted definition request.
#[derive(Debug)]
pub struct DefinitionWaveform {
    descriptor: Arc<DescriptorAllocation>,
    peaks: PeakBuffer,
}

impl DefinitionWaveform {
    pub fn descriptor(&self) -> &WaveformDescriptor {
        &self.descriptor.value
    }
    pub fn measured_end(&self) -> SignalSample {
        SignalSample(i64::try_from(self.peaks.measured_end()).expect("validated waveform extent"))
    }
    pub fn level_count(&self) -> usize {
        self.peaks.level_count()
    }
    pub fn level(&self, level: usize) -> Option<&[StereoExtrema]> {
        self.peaks.level(level)
    }
    pub fn bin_samples(&self, level: usize, index: usize) -> Option<Range<SignalSample>> {
        let range = self.peaks.bin_offsets(level, index)?;
        Some(
            SignalSample(i64::try_from(range.start).ok()?)
                ..SignalSample(i64::try_from(range.end).ok()?),
        )
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
}

/// Pure definition accumulator. Finalization needs no additional peak storage.
pub(crate) struct WaveformBuilder {
    descriptor: Arc<DescriptorAllocation>,
    peaks: PeakAccumulator,
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
        descriptor.leaf_stride = peaks::leaf_stride(total, limits)?;
        let bytes = size_of::<DescriptorAllocation>()
            + ARC_HEADER_BYTES
            + 4 * deadpan_core::MAX_IDENTITY_BYTES;
        let permit = memory.reserve(bytes)?;
        let peaks = PeakAccumulator::new(
            total,
            descriptor.leaf_stride,
            size_of::<DefinitionWaveform>(),
            memory,
        )?;
        Ok(Self {
            descriptor: Arc::new(DescriptorAllocation {
                value: descriptor,
                _permit: permit,
            }),
            peaks,
        })
    }
    pub(crate) fn examined_samples(&self) -> u64 {
        self.peaks.examined_samples()
    }
    pub(crate) fn measured_end(&self) -> SignalSample {
        SignalSample(i64::try_from(self.peaks.measured_end()).expect("validated waveform extent"))
    }
    pub(crate) fn push(
        &mut self,
        start: SignalSample,
        samples: &[[f32; 2]],
    ) -> Result<(), WaveformError> {
        let start = u64::try_from(start.0).map_err(|_| WaveformError::InvalidSequence)?;
        self.peaks.push(start, samples)
    }
    pub(crate) fn snapshot(
        &self,
        memory: &WaveformMemory,
    ) -> Result<Arc<DefinitionWaveform>, WaveformError> {
        Ok(Arc::new(DefinitionWaveform {
            descriptor: self.descriptor.clone(),
            peaks: self.peaks.snapshot(memory)?,
        }))
    }
    pub(crate) fn finish(self, completion: WaveformCompletion) -> WaveformMeasurement {
        let examined_samples = self.peaks.examined_samples();
        WaveformMeasurement {
            waveform: Arc::new(DefinitionWaveform {
                descriptor: self.descriptor,
                peaks: self.peaks.finish(),
            }),
            completion,
            examined_samples,
        }
    }
}

#[cfg(test)]
#[path = "waveform_tests.rs"]
mod tests;
