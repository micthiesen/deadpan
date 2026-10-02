//! Shared bounded aggregation; coordinate interpretation stays with each DTO.

use super::*;

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Level {
    offset: usize,
    pub(super) capacity: usize,
    measured: usize,
    stride: u64,
}

pub(super) fn leaf_stride(total: u64, limits: WaveformLimits) -> Result<u64, WaveformError> {
    let mut stride = MIN_LEAF_STRIDE;
    while total.div_ceil(stride) > u64::from(limits.maximum_leaves) {
        stride = stride.checked_mul(2).ok_or(WaveformError::Overflow)?;
    }
    Ok(stride)
}

#[derive(Debug)]
pub(super) struct PeakBuffer {
    pub(super) levels: [Level; MAX_LEVELS],
    level_count: usize,
    measured_end: u64,
    pub(super) peaks: Vec<StereoExtrema>,
    total: u64,
    stride: u64,
    metadata_bytes: usize,
    _permit: MemoryPermit,
}

impl PeakBuffer {
    fn allocate(
        total: u64,
        stride: u64,
        metadata_bytes: usize,
        memory: &WaveformMemory,
    ) -> Result<Self, WaveformError> {
        let mut remaining = total.div_ceil(stride);
        let mut level_stride = stride;
        let mut levels = [Level::default(); MAX_LEVELS];
        let mut level_count = 0;
        let mut slots = 0usize;
        while remaining > 0 {
            let capacity = usize::try_from(remaining).map_err(|_| WaveformError::Overflow)?;
            let level = levels
                .get_mut(level_count)
                .ok_or(WaveformError::InvalidGeometry)?;
            *level = Level {
                offset: slots,
                capacity,
                measured: 0,
                stride: level_stride,
            };
            slots = slots.checked_add(capacity).ok_or(WaveformError::Overflow)?;
            level_count += 1;
            if remaining == 1 {
                break;
            }
            remaining = remaining.div_ceil(2);
            level_stride = level_stride.checked_mul(2).ok_or(WaveformError::Overflow)?;
        }
        let bytes = slots
            .checked_mul(size_of::<StereoExtrema>())
            .and_then(|bytes| bytes.checked_add(metadata_bytes + ARC_HEADER_BYTES))
            .ok_or(WaveformError::Overflow)?;
        let mut permit = memory.reserve(bytes)?;
        let mut peaks = Vec::new();
        peaks
            .try_reserve_exact(slots)
            .map_err(|_| WaveformError::Allocation)?;
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
            levels,
            level_count,
            measured_end: 0,
            peaks,
            total,
            stride,
            metadata_bytes,
            _permit: permit,
        })
    }

    pub(super) fn measured_end(&self) -> u64 {
        self.measured_end
    }
    pub(super) fn level_count(&self) -> usize {
        self.level_count
    }
    pub(super) fn level(&self, level: usize) -> Option<&[StereoExtrema]> {
        let level = self
            .levels
            .get(level)
            .filter(|_| level < self.level_count)?;
        self.peaks
            .get(level.offset..level.offset.checked_add(level.measured)?)
    }
    pub(super) fn bin_offsets(&self, level: usize, index: usize) -> Option<Range<u64>> {
        let level = self
            .levels
            .get(level)
            .filter(|_| level < self.level_count)?;
        if index >= level.measured {
            return None;
        }
        let start = u64::try_from(index).ok()?.checked_mul(level.stride)?;
        Some(start..start.checked_add(level.stride)?.min(self.total))
    }
}

pub(super) struct PeakAccumulator {
    pub(super) data: PeakBuffer,
    examined: u64,
    pending: Option<StereoExtrema>,
    pending_samples: u64,
}

impl PeakAccumulator {
    pub(super) fn new(
        total: u64,
        stride: u64,
        metadata_bytes: usize,
        memory: &WaveformMemory,
    ) -> Result<Self, WaveformError> {
        Ok(Self {
            data: PeakBuffer::allocate(total, stride, metadata_bytes, memory)?,
            examined: 0,
            pending: None,
            pending_samples: 0,
        })
    }
    pub(super) fn examined_samples(&self) -> u64 {
        self.examined
    }
    pub(super) fn measured_end(&self) -> u64 {
        self.data.measured_end
    }

    /// Reject the whole block before changing partial or published coverage.
    pub(super) fn push(&mut self, start: u64, samples: &[[f32; 2]]) -> Result<(), WaveformError> {
        let count = u64::try_from(samples.len()).map_err(|_| WaveformError::InvalidSamples)?;
        if count == 0
            || count > u64::from(crate::MAX_OUTPUT_FRAMES)
            || samples.iter().flatten().any(|value| !value.is_finite())
        {
            return Err(WaveformError::InvalidSamples);
        }
        let end = self
            .examined
            .checked_add(count)
            .ok_or(WaveformError::Overflow)?;
        if start != self.examined || end > self.data.total {
            return Err(WaveformError::InvalidSequence);
        }
        for &sample in samples {
            let next = StereoExtrema::sample(sample);
            self.pending = Some(self.pending.map_or(next, |prior| prior.merge(next)));
            self.pending_samples += 1;
            self.examined += 1;
            if self.pending_samples == self.data.stride || self.examined == self.data.total {
                self.complete_leaf(self.examined == self.data.total)?;
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
        self.data.measured_end = self.examined;
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
    pub(super) fn snapshot(&self, memory: &WaveformMemory) -> Result<PeakBuffer, WaveformError> {
        let mut copy = PeakBuffer::allocate(
            self.data.total,
            self.data.stride,
            self.data.metadata_bytes,
            memory,
        )?;
        copy.measured_end = self.data.measured_end;
        for index in 0..self.data.level_count {
            let level = self.data.levels[index];
            copy.levels[index].measured = level.measured;
            let range = level.offset..level.offset + level.measured;
            copy.peaks[range.clone()].copy_from_slice(&self.data.peaks[range]);
        }
        Ok(copy)
    }
    pub(super) fn finish(self) -> PeakBuffer {
        self.data
    }
}
