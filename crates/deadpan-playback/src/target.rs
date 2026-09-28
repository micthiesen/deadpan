//! Read-only playback views and exact content/delivery clock mappings.

mod audio_range;
pub use audio_range::AudioRange;

use std::sync::Arc;

use deadpan_core::{
    AssetId, AudioSample, ExactRatio, FrameDuration, FrameRate, NodeId, ProjectDocument,
    ProjectFrame, SourceFrameIndex, SourceNode, SourceQualificationId,
};
use deadpan_store::source_registration::SourceQualificationReceipt;

use crate::{RequestError, Snapshot};

/// Construct on the project service/preparation worker, never on the UI thread.
/// It retains the full A/V union, including audio before and after the picture.
#[derive(Debug)]
pub struct Original {
    asset: AssetId,
    receipt: Arc<SourceQualificationReceipt>,
    rate: FrameRate,
    index: Arc<SourceFrameIndex>,
    source: SourceNode,
    origin: ExactRatio,
    end: AudioSample,
}

impl Original {
    pub fn new(
        rate: FrameRate,
        asset: AssetId,
        receipt: Arc<SourceQualificationReceipt>,
    ) -> Result<Self, String> {
        let timing = receipt
            .snapshot()
            .derive_timing(rate)
            .map_err(|e| e.to_string())?;
        let measured = receipt
            .snapshot()
            .video()
            .ok_or("Original has no qualified picture index")?
            .index()
            .index();
        let index = Arc::new(
            SourceFrameIndex::new(
                asset.clone(),
                measured.time_base(),
                measured.frames().to_vec(),
                measured.terminal_end(),
                measured.terminal_provenance(),
            )
            .map_err(|e| e.to_string())?,
        );
        let end = rate
            .audio_boundary(ProjectFrame(timing.duration.frames()))
            .map_err(|e| e.to_string())?;
        let original = Self {
            asset: asset.clone(),
            receipt,
            rate,
            index,
            source: timing.source_node(asset),
            origin: timing.origin_seconds,
            end,
        };
        let mut previous = AudioSample(0);
        for boundary in 0..=original.frame_count() {
            let sample = original.sample_at_selection_boundary(boundary)?;
            if sample.0 < previous.0 || sample.0 > end.0 {
                return Err(
                    "Original picture boundaries are outside its full source duration".into(),
                );
            }
            previous = sample;
        }
        Ok(original)
    }

    pub fn asset(&self) -> &AssetId {
        &self.asset
    }
    pub fn qualification(&self) -> &SourceQualificationId {
        self.receipt.id()
    }
    pub fn rate(&self) -> FrameRate {
        self.rate
    }
    pub fn index(&self) -> &Arc<SourceFrameIndex> {
        &self.index
    }
    pub fn frame_count(&self) -> u64 {
        self.index.frames().len() as u64
    }
    pub fn duration(&self) -> FrameDuration {
        self.source.duration
    }
    pub fn end(&self) -> AudioSample {
        self.end
    }

    /// Original cursor zero includes leading audio; the final boundary includes
    /// the full measured tail and outward project-frame enclosure.
    pub fn sample_at_boundary(&self, boundary: u64) -> Result<AudioSample, String> {
        sample_at_boundary(&self.index, self.origin, self.end, boundary)
    }

    /// Selected moments use measured picture endpoints. Unlike whole-Original
    /// cursor boundaries, these never add audio lead/tail or enclosure slack.
    pub fn sample_at_selection_boundary(&self, boundary: u64) -> Result<AudioSample, String> {
        sample_at_selection_boundary(&self.index, self.origin, boundary)
    }

    /// Invert the same rounded boundaries used to start audition. Picture zero
    /// holds through leading audio; the last picture holds through the tail.
    pub fn frame_at_sample(&self, sample: AudioSample) -> Result<u64, String> {
        frame_at_sample(&self.index, self.origin, self.end, sample)
    }

    fn document(&self, snapshot: &Snapshot) -> Result<ProjectDocument, String> {
        let entry = snapshot
            .sources
            .get(&self.asset)
            .ok_or("Original receipt is absent from the captured revision")?;
        let asset = snapshot
            .document
            .assets()
            .get(&self.asset)
            .ok_or("Original asset is absent from the captured revision")?;
        if self.rate != snapshot.document.presentation_basis().frame_rate
            || entry.receipt.id() != self.qualification()
            || asset.source_qualification.as_ref() != Some(self.qualification())
            || entry.original.object() != self.receipt.original()
        {
            return Err("Original differs from the captured source contract".into());
        }
        snapshot
            .document
            .source_view(
                &self.asset,
                self.source.clone(),
                NodeId::new("audition-original-root").map_err(|e| e.to_string())?,
                NodeId::new("audition-original-source").map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())
    }
}

fn sample_at_boundary(
    index: &SourceFrameIndex,
    origin: ExactRatio,
    end: AudioSample,
    boundary: u64,
) -> Result<AudioSample, String> {
    if boundary == 0 {
        return Ok(AudioSample(0));
    }
    if boundary == index.frames().len() as u64 {
        return Ok(end);
    }
    sample_at_selection_boundary(index, origin, boundary)
}

fn sample_at_selection_boundary(
    index: &SourceFrameIndex,
    origin: ExactRatio,
    boundary: u64,
) -> Result<AudioSample, String> {
    let ordinal = usize::try_from(boundary).map_err(|e| e.to_string())?;
    let pts = if ordinal == index.frames().len() {
        index.terminal_end()
    } else {
        index
            .frames()
            .get(ordinal)
            .ok_or("Original boundary is out of range")?
            .pts
    };
    let base = index.time_base();
    let sample = ExactRatio::integer(pts)
        .checked_mul(
            ExactRatio::new(i128::from(base.numerator()), i128::from(base.denominator()))
                .map_err(|e| e.to_string())?,
        )
        .and_then(|seconds| seconds.checked_sub(origin))
        .and_then(|seconds| seconds.checked_mul(ExactRatio::integer(48_000)))
        .and_then(ExactRatio::round_even)
        .map_err(|e| e.to_string())?;
    Ok(AudioSample(
        i64::try_from(sample).map_err(|e| e.to_string())?,
    ))
}

fn frame_at_sample(
    index: &SourceFrameIndex,
    origin: ExactRatio,
    end: AudioSample,
    sample: AudioSample,
) -> Result<u64, String> {
    if sample.0 < 0 || sample.0 > end.0 {
        return Err("Original sample is out of range".into());
    }
    if sample == end {
        return Ok(index.frames().len() as u64);
    }
    let mut low = 0;
    let mut high = index.frames().len() as u64;
    while low + 1 < high {
        let mid = low + (high - low) / 2;
        if sample_at_boundary(index, origin, end, mid)?.0 <= sample.0 {
            low = mid;
        } else {
            high = mid;
        }
    }
    Ok(low)
}

impl PartialEq for Original {
    fn eq(&self, other: &Self) -> bool {
        self.asset == other.asset
            && self.rate == other.rate
            && self.qualification() == other.qualification()
    }
}
impl Eq for Original {}

/// A qualified audio-only catalog view. Construct on the project service or
/// preparation worker; the descriptor owns no decoder, device or authored edit.
/// Its sample zero is the first measured available source sample, including any
/// priming for which no explicit exclusion evidence exists.
#[derive(Debug)]
pub struct Sound {
    asset: AssetId,
    receipt: Arc<SourceQualificationReceipt>,
    rate: FrameRate,
    source: SourceNode,
    end: AudioSample,
}

impl Sound {
    pub fn new(
        rate: FrameRate,
        asset: AssetId,
        receipt: Arc<SourceQualificationReceipt>,
    ) -> Result<Self, String> {
        if receipt.snapshot().video().is_some() || receipt.snapshot().audio().is_none() {
            return Err("Sound requires a qualified audio-only source".into());
        }
        let timing = receipt
            .snapshot()
            .derive_timing(rate)
            .map_err(|error| error.to_string())?;
        let span = timing
            .audio
            .ok_or("Sound has no measured available audio")?
            .span;
        // The beat encloses the exact source duration with whole project frames.
        // Audition ends at the source boundary itself, without that frame slack.
        let ticks = i128::from(span.end().ticks) - i128::from(span.start().ticks);
        let base = span.start().time_base;
        let seconds_per_tick =
            ExactRatio::new(i128::from(base.numerator()), i128::from(base.denominator()))
                .map_err(|error| error.to_string())?;
        let samples = ExactRatio::new(ticks, 1)
            .and_then(|ticks| ticks.checked_mul(seconds_per_tick))
            .and_then(|seconds| {
                seconds.checked_mul(ExactRatio::integer(i64::from(
                    deadpan_core::MIX_SAMPLE_RATE,
                )))
            })
            .and_then(ExactRatio::round_even)
            .map_err(|error| error.to_string())?;
        let end = AudioSample(i64::try_from(samples).map_err(|error| error.to_string())?);
        if end.0 <= 0 {
            return Err("Sound has no duration on the mix sample clock".into());
        }
        Ok(Self {
            asset: asset.clone(),
            receipt,
            rate,
            source: timing.source_node(asset),
            end,
        })
    }

    pub fn asset(&self) -> &AssetId {
        &self.asset
    }

    pub fn qualification_id(&self) -> &SourceQualificationId {
        self.receipt.id()
    }

    pub fn rate(&self) -> FrameRate {
        self.rate
    }

    pub fn duration_samples(&self) -> AudioSample {
        self.end
    }

    fn document(&self, snapshot: &Snapshot) -> Result<ProjectDocument, String> {
        let entry = snapshot
            .sources
            .get(&self.asset)
            .ok_or("Sound receipt is absent from the captured revision")?;
        let asset = snapshot
            .document
            .assets()
            .get(&self.asset)
            .ok_or("Sound asset is absent from the captured revision")?;
        if self.rate != snapshot.document.presentation_basis().frame_rate
            || entry.receipt.id() != self.qualification_id()
            || asset.source_qualification.as_ref() != Some(self.qualification_id())
            || asset.content_hash != self.receipt.original().content().to_string()
            || asset.audio != self.source.audio.as_ref().map(|audio| audio.span)
            || asset.video.is_some()
            || asset.still_image
            || asset.frame_count.is_some()
            || entry.original.object() != self.receipt.original()
            || entry.original.sha256() != self.receipt.snapshot().content().sha256()
        {
            return Err("Sound differs from the captured source contract".into());
        }
        snapshot
            .document
            .source_view(
                &self.asset,
                self.source.clone(),
                NodeId::new("audition-sound-root").map_err(|error| error.to_string())?,
                NodeId::new("audition-sound-source").map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())
    }
}

impl PartialEq for Sound {
    fn eq(&self, other: &Self) -> bool {
        self.asset == other.asset
            && self.rate == other.rate
            && self.qualification_id() == other.qualification_id()
    }
}
impl Eq for Sound {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Sequence,
    Original(Arc<Original>),
    Sound(Arc<Sound>),
    AudioRange(Arc<AudioRange>),
}

impl Target {
    pub(crate) fn document(&self, snapshot: &Snapshot) -> Result<Arc<ProjectDocument>, String> {
        match self {
            Self::Sequence => Ok(snapshot.document.clone()),
            Self::Original(original) => original.document(snapshot).map(Arc::new),
            Self::Sound(sound) => sound.document(snapshot).map(Arc::new),
            Self::AudioRange(range) => range.document(snapshot).map(Arc::new),
        }
    }

    pub(crate) fn effective_end(&self, plan_end: AudioSample) -> Result<AudioSample, String> {
        let end = match self {
            Self::Sound(sound) => sound.duration_samples(),
            Self::AudioRange(range) => range.duration_samples(),
            Self::Sequence | Self::Original(_) => plan_end,
        };
        if end.0 < 0 || end > plan_end {
            return Err("Audition target exceeds its canonical audio plan".into());
        }
        Ok(end)
    }
}

/// An immutable half-open content interval. Device coordinates keep increasing
/// across loop seams; mapping them back never changes the canonical DSP plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    start: AudioSample,
    end: AudioSample,
    looping: bool,
}

impl Window {
    pub fn new(start: AudioSample, end: AudioSample, looping: bool) -> Result<Self, RequestError> {
        if start.0 < 0 || end.0 < start.0 || (looping && end == start) {
            return Err(RequestError::InvalidWindow);
        }
        Ok(Self {
            start,
            end,
            looping,
        })
    }
    pub fn start(self) -> AudioSample {
        self.start
    }
    pub fn end(self) -> AudioSample {
        self.end
    }
    pub fn looping(self) -> bool {
        self.looping
    }
    pub fn sample(self, delivery: AudioSample) -> Option<AudioSample> {
        let offset = delivery
            .0
            .checked_sub(self.start.0)
            .filter(|offset| *offset >= 0)?;
        if self.looping {
            Some(AudioSample(
                self.start.0 + offset % (self.end.0 - self.start.0),
            ))
        } else {
            (delivery.0 <= self.end.0).then_some(delivery)
        }
    }
    pub fn lap(self, delivery: AudioSample) -> Option<u64> {
        self.sample(delivery)?;
        if self.looping {
            u64::try_from((delivery.0 - self.start.0) / (self.end.0 - self.start.0)).ok()
        } else {
            Some(0)
        }
    }
    pub(crate) fn validate(self, end: AudioSample, delivery: AudioSample) -> Result<(), String> {
        if self.end.0 > end.0 {
            return Err("audition window is past the target end".into());
        }
        if self.sample(delivery).is_none() {
            return Err("playback start is outside the audition window".into());
        }
        Ok(())
    }
    pub(crate) fn delivery_end(self) -> AudioSample {
        if self.looping {
            AudioSample(i64::MAX)
        } else {
            self.end
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadpan_core::{IndexedSourceFrame, SourceFrameId, SourceTimeBase, TerminalProvenance};

    #[test]
    fn signed_origin_and_half_sample_ties_use_the_same_forward_and_inverse_clock() {
        let index = SourceFrameIndex::new(
            AssetId::new("original").unwrap(),
            SourceTimeBase::new(1, 96_000).unwrap(),
            [-98, -95, -94, -91]
                .into_iter()
                .enumerate()
                .map(|(number, pts)| IndexedSourceFrame {
                    identity: SourceFrameId(number as u64),
                    pts,
                    reported_duration: Some(1),
                    keyframe: true,
                    seek_from: None,
                    decode_timestamp: None,
                })
                .collect(),
            -89,
            TerminalProvenance::DecodedFrameDuration,
        )
        .unwrap();
        let origin = ExactRatio::new(-100, 96_000).unwrap();
        let end = AudioSample(8);
        for (boundary, expected) in [0, 2, 3, 4, 8].into_iter().enumerate() {
            assert_eq!(
                sample_at_boundary(&index, origin, end, boundary as u64).unwrap(),
                AudioSample(expected)
            );
        }
        for (sample, expected) in [0, 0, 1, 2, 3, 3, 3, 3, 4].into_iter().enumerate() {
            assert_eq!(
                frame_at_sample(&index, origin, end, AudioSample(sample as i64)).unwrap(),
                expected
            );
        }
        assert!(frame_at_sample(&index, origin, end, AudioSample(-1)).is_err());
        assert!(sample_at_boundary(&index, origin, end, 5).is_err());
    }
}
