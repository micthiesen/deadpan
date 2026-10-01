//! Qualified audio selections keep their full source support on a local clock.

use super::*;
use deadpan_core::{
    ExactFrameRange, LinkRelation, SourceAudio, SourceAudioMapping, SourceEditWindow, SourceSpan,
    SourceVideo, SourceVideoMapping,
};

/// An audio-only audition of an explicit range from a qualified Original or
/// catalog source. Construct on the project service/preparation worker, never
/// on the UI thread. Sample zero is the selection's first original sample.
#[derive(Debug)]
pub struct AudioRange {
    asset: AssetId,
    receipt: Arc<SourceQualificationReceipt>,
    rate: FrameRate,
    span: SourceSpan,
    source: SourceNode,
    end: AudioSample,
    video: Option<SourceSpan>,
    frame_count: Option<FrameDuration>,
}

impl AudioRange {
    pub fn new(
        rate: FrameRate,
        asset: AssetId,
        receipt: Arc<SourceQualificationReceipt>,
        span: SourceSpan,
    ) -> Result<Self, String> {
        let measured = receipt
            .snapshot()
            .audio()
            .ok_or("Audio range requires qualified audio")?;
        let timing = receipt
            .snapshot()
            .derive_timing(rate)
            .map_err(|error| error.to_string())?;
        let full = timing
            .audio
            .ok_or("Audio range has no available samples")?
            .span;
        let sample_clock = deadpan_core::SourceTimeBase::new(1, measured.stream().sample_rate)
            .map_err(|error| error.to_string())?;
        if span.start().time_base != sample_clock
            || span.end().time_base != sample_clock
            || !full.contains_span(span)
        {
            return Err(
                "Audio range must use whole original samples within measured coverage".into(),
            );
        }
        let samples = i128::from(span.end().ticks) - i128::from(span.start().ticks);
        let seconds = ExactRatio::new(samples, i128::from(measured.stream().sample_rate))
            .map_err(|error| error.to_string())?;
        let end = seconds
            .checked_mul(ExactRatio::integer(i64::from(
                deadpan_core::MIX_SAMPLE_RATE,
            )))
            .and_then(ExactRatio::round_even)
            .map_err(|error| error.to_string())?;
        let end = AudioSample(i64::try_from(end).map_err(|error| error.to_string())?);
        if end.0 <= 0 {
            return Err("Audio range has no duration on the mix sample clock".into());
        }
        let frames_per_second =
            ExactRatio::new(i128::from(rate.numerator()), i128::from(rate.denominator()))
                .map_err(|error| error.to_string())?;
        let selected_frames = seconds
            .checked_mul(frames_per_second)
            .map_err(|error| error.to_string())?;
        let edit_window = SourceEditWindow::new(ExactRatio::ZERO, selected_frames)
            .map_err(|error| error.to_string())?;
        let duration = selected_frames
            .ceil()
            .and_then(|frames| i64::try_from(frames).map_err(|_| deadpan_core::TimeError::Overflow))
            .and_then(FrameDuration::new)
            .map_err(|error| error.to_string())?;
        edit_window
            .validate(duration)
            .map_err(|error| error.to_string())?;
        let start = ExactRatio::new(
            i128::from(full.start().ticks) - i128::from(span.start().ticks),
            i128::from(measured.stream().sample_rate),
        )
        .and_then(|seconds| seconds.checked_mul(frames_per_second))
        .map_err(|error| error.to_string())?;
        let frames = SourceAudioMapping::natural_rate(full, rate)
            .and_then(|mapping| mapping.duration_frames(duration))
            .map_err(|error| error.to_string())?;
        let selection = ExactFrameRange::new(ExactRatio::ZERO, selected_frames)
            .map_err(|error| error.to_string())?;
        let source = SourceNode {
            duration,
            edit_window: Some(edit_window),
            video: SourceVideo::Blank,
            video_mapping: SourceVideoMapping::FitBeat,
            audio: Some(SourceAudio {
                asset: asset.clone(),
                span: full,
            }),
            audio_mapping: SourceAudioMapping::SelectedPlacement {
                start,
                frames,
                selection,
            },
            audio_offset: AudioSample(0),
            link: LinkRelation::Independent,
        };
        let frame_count = receipt
            .snapshot()
            .video()
            .map(|video| {
                i64::try_from(video.index().index().frames().len())
                    .map_err(|_| deadpan_core::TimeError::Overflow)
                    .and_then(FrameDuration::new)
            })
            .transpose()
            .map_err(|error| error.to_string())?;
        Ok(Self {
            asset,
            receipt,
            rate,
            span,
            source,
            end,
            video: timing.video.map(|video| video.span),
            frame_count,
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

    pub fn span(&self) -> SourceSpan {
        self.span
    }

    pub fn duration_samples(&self) -> AudioSample {
        self.end
    }

    pub(super) fn document(&self, snapshot: &Snapshot) -> Result<ProjectDocument, String> {
        let entry = snapshot
            .sources
            .get(&self.asset)
            .ok_or("Audio range receipt is absent from the captured revision")?;
        let asset = snapshot
            .document
            .assets()
            .get(&self.asset)
            .ok_or("Audio range asset is absent from the captured revision")?;
        if self.rate != snapshot.document.presentation_basis().frame_rate
            || entry.receipt.id() != self.qualification_id()
            || asset.source_qualification.as_ref() != Some(self.qualification_id())
            || asset.content_hash != self.receipt.original().content().to_string()
            || asset.audio != self.source.audio.as_ref().map(|audio| audio.span)
            || asset.video != self.video
            || asset.still_image
            || asset.frame_count != self.frame_count
            || entry.original.object() != self.receipt.original()
            || entry.original.sha256() != self.receipt.snapshot().content().sha256()
        {
            return Err("Audio range differs from the captured source contract".into());
        }
        snapshot
            .document
            .source_view(
                &self.asset,
                self.source.clone(),
                NodeId::new("audition-audio-range-root").map_err(|error| error.to_string())?,
                NodeId::new("audition-audio-range-source").map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())
    }
}

impl PartialEq for AudioRange {
    fn eq(&self, other: &Self) -> bool {
        self.asset == other.asset
            && self.rate == other.rate
            && self.span == other.span
            && self.qualification_id() == other.qualification_id()
    }
}

impl Eq for AudioRange {}
