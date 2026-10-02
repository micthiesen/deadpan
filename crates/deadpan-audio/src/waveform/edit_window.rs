//! Absolute root-sample peaks. No definition clock or shifted clip is invented.

use super::*;
use deadpan_core::AudioSample;
use deadpan_plan::AudioBoundaryRule;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditWaveformStage {
    /// Complete implemented time/pitch, voice edges, owner gain and group/root
    /// mix. The common limiter, mastering and monitor gain are excluded.
    AuthoredBusBeforeLimiter,
}

impl EditWaveformStage {
    pub const fn label(self) -> &'static str {
        match self {
            Self::AuthoredBusBeforeLimiter => "authored_bus_pcm_before_limiter",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditWaveformDescriptor {
    pub stage: EditWaveformStage,
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub root: NodeId,
    pub frame_rate: FrameRate,
    pub project_duration: FrameDuration,
    pub sample_rate: u32,
    pub grid: AudioSampleGrid<AudioSample>,
    /// Exact caller-requested half-open absolute project sample interval.
    pub samples: Range<AudioSample>,
    pub leaf_stride: u64,
}

#[derive(Debug)]
struct Descriptor {
    value: EditWaveformDescriptor,
    _permit: MemoryPermit,
}

#[derive(Debug)]
pub struct EditWaveform {
    descriptor: Arc<Descriptor>,
    peaks: PeakBuffer,
}

impl EditWaveform {
    pub fn descriptor(&self) -> &EditWaveformDescriptor {
        &self.descriptor.value
    }
    /// Absolute end of completely measured bins, initially samples.start.
    pub fn measured_end(&self) -> AudioSample {
        AudioSample(
            self.descriptor().samples.start.0
                + i64::try_from(self.peaks.measured_end()).expect("validated sample window"),
        )
    }
    pub fn level_count(&self) -> usize {
        self.peaks.level_count()
    }
    pub fn level(&self, level: usize) -> Option<&[StereoExtrema]> {
        self.peaks.level(level)
    }
    pub fn bin_samples(&self, level: usize, index: usize) -> Option<Range<AudioSample>> {
        let range = self.peaks.bin_offsets(level, index)?;
        let origin = self.descriptor().samples.start.0;
        Some(
            AudioSample(origin.checked_add(i64::try_from(range.start).ok()?)?)
                ..AudioSample(origin.checked_add(i64::try_from(range.end).ok()?)?),
        )
    }
    /// Exact sample-grid coordinates; arbitrary sample windows are never
    /// stretched onto whole frames or normalized onto a new origin.
    pub fn bin_project_frames(
        &self,
        level: usize,
        index: usize,
    ) -> Result<Range<ExactRatio>, WaveformError> {
        let samples = self
            .bin_samples(level, index)
            .ok_or(WaveformError::InvalidGeometry)?;
        Ok(self.descriptor().grid.at(samples.start)?..self.descriptor().grid.at(samples.end)?)
    }
}

#[derive(Debug)]
pub struct EditWaveformMeasurement {
    pub waveform: Arc<EditWaveform>,
    pub completion: WaveformCompletion,
    /// Relative count, including an incomplete unpublished leaf.
    pub examined_samples: u64,
}

pub(crate) struct EditWaveformBuilder {
    descriptor: Arc<Descriptor>,
    peaks: PeakAccumulator,
}

impl EditWaveformBuilder {
    pub(crate) fn new(
        mut descriptor: EditWaveformDescriptor,
        limits: WaveformLimits,
        memory: &WaveformMemory,
    ) -> Result<Self, WaveformError> {
        let samples = &descriptor.samples;
        let expected_step = ExactRatio::new(
            i128::from(descriptor.frame_rate.numerator()),
            i128::from(descriptor.frame_rate.denominator())
                * i128::from(deadpan_core::MIX_SAMPLE_RATE),
        )?;
        if samples.start.0 < 0
            || samples.end < samples.start
            || descriptor.sample_rate != deadpan_core::MIX_SAMPLE_RATE
            || descriptor.grid.boundary_rule() != AudioBoundaryRule::RoundEven
            || descriptor.grid.frame_origin() != ExactRatio::ZERO
            || descriptor.grid.frames_per_sample() != expected_step
            || samples.end
                > descriptor
                    .frame_rate
                    .audio_boundary(deadpan_core::ProjectFrame(
                        descriptor.project_duration.frames(),
                    ))?
        {
            return Err(WaveformError::InvalidGeometry);
        }
        let total = u64::try_from(samples.end.0 - samples.start.0)
            .map_err(|_| WaveformError::InvalidGeometry)?;
        descriptor.leaf_stride = peaks::leaf_stride(total, limits)?;
        let permit = memory.reserve(
            size_of::<Descriptor>() + ARC_HEADER_BYTES + 3 * deadpan_core::MAX_IDENTITY_BYTES,
        )?;
        let peaks = PeakAccumulator::new(
            total,
            descriptor.leaf_stride,
            size_of::<EditWaveform>(),
            memory,
        )?;
        Ok(Self {
            descriptor: Arc::new(Descriptor {
                value: descriptor,
                _permit: permit,
            }),
            peaks,
        })
    }
    pub(crate) fn examined_samples(&self) -> u64 {
        self.peaks.examined_samples()
    }
    pub(crate) fn measured_end(&self) -> AudioSample {
        AudioSample(
            self.descriptor.value.samples.start.0
                + i64::try_from(self.peaks.measured_end()).expect("validated sample window"),
        )
    }
    pub(crate) fn push(
        &mut self,
        start: AudioSample,
        samples: &[[f32; 2]],
    ) -> Result<(), WaveformError> {
        let offset = start
            .0
            .checked_sub(self.descriptor.value.samples.start.0)
            .ok_or(WaveformError::InvalidSequence)?;
        self.peaks.push(
            u64::try_from(offset).map_err(|_| WaveformError::InvalidSequence)?,
            samples,
        )
    }
    pub(crate) fn snapshot(
        &self,
        memory: &WaveformMemory,
    ) -> Result<Arc<EditWaveform>, WaveformError> {
        Ok(Arc::new(EditWaveform {
            descriptor: self.descriptor.clone(),
            peaks: self.peaks.snapshot(memory)?,
        }))
    }
    pub(crate) fn finish(self, completion: WaveformCompletion) -> EditWaveformMeasurement {
        let examined_samples = self.peaks.examined_samples();
        EditWaveformMeasurement {
            waveform: Arc::new(EditWaveform {
                descriptor: self.descriptor,
                peaks: self.peaks.finish(),
            }),
            completion,
            examined_samples,
        }
    }
}

#[cfg(test)]
mod tests;
