//! Pure import timing and presentation-basis candidates from measured indexes.
//!
//! These values grant no readiness, byte ownership, document adoption, or
//! editorial-origin authority. In particular, unavailable priming evidence is
//! not permission to remove samples. Original coordinates survive unchanged.

use std::collections::BTreeMap;
use std::ops::Range;

use deadpan_core::{
    AssetId, AudioSample, ColorPolicy, DocumentError, EndpointPolicy, ExactFrameRange, ExactRatio,
    FrameDuration, FrameRate, LinkRelation, PresentationBasis, SourceAudio, SourceAudioMapping,
    SourceEditWindow, SourceFrameId, SourceNode, SourceSpan, SourceTimeBase, SourceTimestamp,
    SourceVideo, SourceVideoMapping, TerminalProvenance, TimeError,
};
use deadpan_source::{ColorTransfer, SourceStreamInfo};

use crate::audio_index::AudioIndexSnapshot;
use crate::source_index::SourceIndexSnapshot;

/// A small histogram bound independent of the maximum number of indexed frames.
pub const MAX_CADENCE_INTERVALS: usize = 256;
/// Each even raster dimension may differ by at most one display pixel. This
/// exact per-axis envelope also covers tiny sources where a percentage bound
/// would reject ordinary codec-legal rounding. The resulting aspect error is
/// retained explicitly in `GeometryEvidence`.
pub const MAX_RASTER_AXIS_ERROR_PIXELS: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum ImportTimingError {
    #[error("at least one measured selected stream is required")]
    NoStreams,
    #[error("selected indexes do not identify different streams of one original")]
    StreamMismatch,
    #[error("audio coverage is empty or contains an unavailable interior interval")]
    UnavailableAudio,
    #[error("video requires a measured positive decoded terminal duration")]
    UnmeasuredVideoEnd,
    #[error("source moment requires a nonempty half-open range of measured picture ordinals")]
    InvalidMomentRange,
    #[error("source metadata does not match the measured video index")]
    VideoMetadataMismatch,
    #[error("observed cadence is ambiguous or exceeds the bounded cadence policy")]
    AmbiguousCadence,
    #[error("display geometry cannot satisfy the bounded even-raster policy")]
    UnsupportedGeometry,
    #[error(transparent)]
    Time(#[from] TimeError),
    #[error(transparent)]
    Document(#[from] DocumentError),
}

/// Includes every contiguous measured available sample. Explicit decoder skip,
/// discard and duration evidence is already reflected by the audio index.
/// Unknown priming stays present; codec padding and container start are unused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportAudioPolicy {
    MeasuredAvailableCoverage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamPlacement {
    pub span: SourceSpan,
    pub start_frames: ExactRatio,
    pub duration_frames: ExactRatio,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportTiming {
    /// Minimum selected stream start in the original shared timestamp clock.
    /// This is an alignment origin, not a claim about editorial priming.
    pub origin_seconds: ExactRatio,
    pub project_rate: FrameRate,
    pub video: Option<StreamPlacement>,
    pub audio: Option<StreamPlacement>,
    /// Ceiling of the exact union extent. Full-source outward enclosure keeps
    /// both tails; text-input nearest-frame rounding is a separate policy.
    pub duration: FrameDuration,
    /// Exact selected A/V union before integral frame enclosure.
    pub edit_window: SourceEditWindow,
    /// Hold the selected picture endpoints over lead, tail and rounding slack.
    pub video_endpoints: EndpointPolicy,
    pub audio_policy: ImportAudioPolicy,
}

impl ImportTiming {
    /// Construct candidate authored intent. The caller still owns asset
    /// registration, stream qualification, and an explicit document transaction.
    pub fn source_node(&self, asset: AssetId) -> SourceNode {
        SourceNode {
            duration: self.duration,
            edit_window: Some(self.edit_window),
            video: self
                .video
                .map_or(SourceVideo::Blank, |placement| SourceVideo::Stream {
                    asset: asset.clone(),
                    span: placement.span,
                }),
            video_mapping: self.video.map_or(SourceVideoMapping::FitBeat, |placement| {
                SourceVideoMapping::Placement {
                    start: placement.start_frames,
                    frames: placement.duration_frames,
                    endpoints: self.video_endpoints,
                }
            }),
            audio: self.audio.map(|placement| SourceAudio {
                asset,
                span: placement.span,
            }),
            audio_mapping: self.audio.map_or(SourceAudioMapping::FitBeat, |placement| {
                SourceAudioMapping::Placement {
                    start: placement.start_frames,
                    frames: placement.duration_frames,
                }
            }),
            link: if self.video.is_some() && self.audio.is_some() {
                LinkRelation::Linked
            } else {
                LinkRelation::Independent
            },
            // Fractional cross-clock placement belongs to the exact mapping,
            // never an independently rounded 48 kHz alignment offset.
            audio_offset: AudioSample(0),
        }
    }
}

/// Full measured audio mapping with an exact audible selection in local frames.
/// Original sample endpoints remain integral; selection never refits their rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectedAudioPlacement {
    pub placement: StreamPlacement,
    pub selection: ExactFrameRange,
}

/// A pure selected-picture timing candidate, not registration or edit authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceMomentTiming {
    /// Nonempty half-open original presentation ordinals, including the final
    /// measured boundary when `end` equals the index's frame count.
    pub ordinals: Range<u64>,
    pub origin_seconds: ExactRatio,
    pub project_rate: FrameRate,
    /// Selected picture span, retained for the Original range and its duration.
    pub video: StreamPlacement,
    /// Complete measured picture context with the same affine clock. Keeping
    /// this separate from selection makes source handles available without
    /// changing cadence or exposing adjacent pictures in rounded end slack.
    pub video_context: StreamPlacement,
    /// Visible half-open window in the full picture context's local clock.
    pub video_selection: ExactFrameRange,
    /// None means no measured audio stream. A stream with no overlap remains
    /// linked with equal selection endpoints and its complete affine context.
    pub audio: Option<SelectedAudioPlacement>,
    /// Ceiling of the selected picture duration, rounded only once. Picture
    /// holds its selected endpoint in the slack; audio remains cropped exactly.
    pub duration: FrameDuration,
    /// Exact selected picture interval, independent of rounded duration and
    /// any audible, dormant, or absent audio support.
    pub edit_window: SourceEditWindow,
}

impl SourceMomentTiming {
    /// Construct candidate intent referring to an asset admitted by the host.
    pub fn source_node(&self, asset: AssetId) -> SourceNode {
        SourceNode {
            duration: self.duration,
            edit_window: Some(self.edit_window),
            video: SourceVideo::Stream {
                asset: asset.clone(),
                span: self.video_context.span,
            },
            video_mapping: SourceVideoMapping::SelectedPlacement {
                start: self.video_context.start_frames,
                frames: self.video_context.duration_frames,
                selection: self.video_selection,
                endpoints: EndpointPolicy::HoldAdjacent,
            },
            audio: self.audio.map(|audio| SourceAudio {
                asset,
                span: audio.placement.span,
            }),
            audio_mapping: self.audio.map_or(SourceAudioMapping::FitBeat, |audio| {
                SourceAudioMapping::SelectedPlacement {
                    start: audio.placement.start_frames,
                    frames: audio.placement.duration_frames,
                    selection: audio.selection,
                }
            }),
            link: if self.audio.is_some() {
                LinkRelation::Linked
            } else {
                LinkRelation::Independent
            },
            audio_offset: AudioSample(0),
        }
    }
}

/// Select original picture intervals without changing their cadence or audio
/// phase. This shares full-import evidence checks, including complete contiguous
/// available audio coverage and measured video terminal duration. It performs
/// no I/O and grants no media admission or permission to mutate a document.
pub fn derive_source_moment(
    video: &SourceIndexSnapshot,
    audio: Option<&AudioIndexSnapshot>,
    ordinals: Range<u64>,
    project_rate: FrameRate,
) -> Result<SourceMomentTiming, ImportTimingError> {
    let index = video.index();
    let frame_count = u64::try_from(index.frames().len()).map_err(|_| TimeError::Overflow)?;
    if ordinals.start >= ordinals.end || ordinals.end > frame_count {
        return Err(ImportTimingError::InvalidMomentRange);
    }
    let (measured_video, measured_audio) = measured_spans(Some(video), audio)?;
    let start = index.interval(SourceFrameId(ordinals.start))?.0;
    let end = index.interval(SourceFrameId(ordinals.end - 1))?.1;
    let selected = SourceSpan::new(
        SourceTimestamp {
            ticks: start,
            time_base: index.time_base(),
        },
        SourceTimestamp {
            ticks: end,
            time_base: index.time_base(),
        },
    )?;
    let origin_seconds = seconds(selected.start())?;
    let video = stream_placement(selected, origin_seconds, project_rate)?;
    let video_context = stream_placement(
        measured_video.ok_or(ImportTimingError::NoStreams)?,
        origin_seconds,
        project_rate,
    )?;
    let edit_window = SourceEditWindow::new(ExactRatio::ZERO, video.duration_frames)?;
    let duration = FrameDuration::new(
        i64::try_from(video.duration_frames.ceil()?).map_err(|_| TimeError::Overflow)?,
    )?;
    edit_window.validate(duration)?;
    let video_selection = ExactFrameRange::new(ExactRatio::ZERO, video.duration_frames)?;
    SourceVideoMapping::SelectedPlacement {
        start: video_context.start_frames,
        frames: video_context.duration_frames,
        selection: video_selection,
        endpoints: EndpointPolicy::HoldAdjacent,
    }
    .selection_in_source(video_context.span, duration)?;
    let audio = measured_audio
        .map(
            |span| -> Result<SelectedAudioPlacement, ImportTimingError> {
                let placement = stream_placement(span, origin_seconds, project_rate)?;
                let end = placement
                    .start_frames
                    .checked_add(placement.duration_frames)?;
                let selected_start = if placement.start_frames.compare_integer(0).is_gt() {
                    placement.start_frames
                } else {
                    ExactRatio::ZERO
                };
                let selected_end = if end.compare(video.duration_frames).is_lt() {
                    end
                } else {
                    video.duration_frames
                };
                let selection = if selected_start.compare(selected_end).is_lt() {
                    ExactFrameRange::new(selected_start, selected_end)?
                } else {
                    // Keep an explicit empty window at the closest point of the
                    // full audio mapping to the exact picture selection. Never
                    // let audio in rounded picture slack become audible.
                    let point = if end.compare_integer(0).is_le() {
                        end
                    } else {
                        placement.start_frames
                    };
                    ExactFrameRange {
                        start: point,
                        end: point,
                    }
                };
                // Check authored mapping bounds even when a small window hides a
                // distant original origin or a much longer measured audio stream.
                SourceAudioMapping::SelectedPlacement {
                    start: placement.start_frames,
                    frames: placement.duration_frames,
                    selection,
                }
                .duration_frames(duration)?;
                Ok(SelectedAudioPlacement {
                    placement,
                    selection,
                })
            },
        )
        .transpose()?;
    Ok(SourceMomentTiming {
        ordinals,
        origin_seconds,
        project_rate,
        video,
        video_context,
        video_selection,
        audio,
        duration,
        edit_window,
    })
}

/// Select complete original audio samples inside measured picture intervals.
/// The first boundary rounds inward with ceil and the last with floor, then
/// both intersect the full contiguous measured audio coverage. No project
/// clock, nominal frame rate, priming assumption, or audio tail is introduced.
pub fn derive_source_audio_moment(
    video: &SourceIndexSnapshot,
    audio: &AudioIndexSnapshot,
    ordinals: Range<u64>,
) -> Result<SourceSpan, ImportTimingError> {
    let index = video.index();
    let count = u64::try_from(index.frames().len()).map_err(|_| TimeError::Overflow)?;
    if ordinals.start >= ordinals.end || ordinals.end > count {
        return Err(ImportTimingError::InvalidMomentRange);
    }
    let (_, measured) = measured_spans(Some(video), Some(audio))?;
    let measured = measured.ok_or(ImportTimingError::UnavailableAudio)?;
    let sample = |ticks| -> Result<ExactRatio, TimeError> {
        seconds(SourceTimestamp {
            ticks,
            time_base: index.time_base(),
        })?
        .checked_mul(ExactRatio::integer(i64::from(audio.stream().sample_rate)))
    };
    let start = sample(index.interval(SourceFrameId(ordinals.start))?.0)?.ceil()?;
    let end = sample(index.interval(SourceFrameId(ordinals.end - 1))?.1)?.floor();
    let start = i64::try_from(start)
        .map_err(|_| TimeError::Overflow)?
        .max(measured.start().ticks);
    let end = i64::try_from(end)
        .map_err(|_| TimeError::Overflow)?
        .min(measured.end().ticks);
    if start >= end {
        return Err(ImportTimingError::UnavailableAudio);
    }
    Ok(SourceSpan::new(
        SourceTimestamp {
            ticks: start,
            time_base: measured.start().time_base,
        },
        SourceTimestamp {
            ticks: end,
            time_base: measured.end().time_base,
        },
    )?)
}

pub fn derive_import_timing(
    video: Option<&SourceIndexSnapshot>,
    audio: Option<&AudioIndexSnapshot>,
    project_rate: FrameRate,
) -> Result<ImportTiming, ImportTimingError> {
    let (video, audio) = measured_spans(video, audio)?;
    let mut spans = video.into_iter().chain(audio);
    let first = spans.next().ok_or(ImportTimingError::NoStreams)?;
    let mut origin = seconds(first.start())?;
    let mut end = seconds(first.end())?;
    for span in spans {
        let start = seconds(span.start())?;
        let candidate_end = seconds(span.end())?;
        if start.checked_sub(origin)?.numerator() < 0 {
            origin = start;
        }
        if candidate_end.checked_sub(end)?.numerator() > 0 {
            end = candidate_end;
        }
    }
    let extent = end
        .checked_sub(origin)?
        .checked_mul(rate_ratio(project_rate)?)?;
    let edit_window = SourceEditWindow::new(ExactRatio::ZERO, extent)?;
    let duration =
        FrameDuration::new(i64::try_from(extent.ceil()?).map_err(|_| TimeError::Overflow)?)?;
    edit_window.validate(duration)?;
    Ok(ImportTiming {
        origin_seconds: origin,
        project_rate,
        video: video
            .map(|span| stream_placement(span, origin, project_rate))
            .transpose()?,
        audio: audio
            .map(|span| stream_placement(span, origin, project_rate))
            .transpose()?,
        duration,
        edit_window,
        video_endpoints: EndpointPolicy::HoldAdjacent,
        audio_policy: ImportAudioPolicy::MeasuredAvailableCoverage,
    })
}

fn measured_spans(
    video: Option<&SourceIndexSnapshot>,
    audio: Option<&AudioIndexSnapshot>,
) -> Result<(Option<SourceSpan>, Option<SourceSpan>), ImportTimingError> {
    if let (Some(video), Some(audio)) = (video, audio)
        && (video.content() != audio.content()
            || video.stream_index() == audio.stream().stream_index)
    {
        return Err(ImportTimingError::StreamMismatch);
    }
    let video = video.map(video_span).transpose()?;
    let audio = audio.map(audio_span).transpose()?;
    Ok((video, audio))
}

fn stream_placement(
    span: SourceSpan,
    origin: ExactRatio,
    project_rate: FrameRate,
) -> Result<StreamPlacement, ImportTimingError> {
    let rate = rate_ratio(project_rate)?;
    Ok(StreamPlacement {
        span,
        start_frames: seconds(span.start())?
            .checked_sub(origin)?
            .checked_mul(rate)?,
        duration_frames: seconds(span.end())?
            .checked_sub(seconds(span.start())?)?
            .checked_mul(rate)?,
    })
}

fn seconds(timestamp: SourceTimestamp) -> Result<ExactRatio, TimeError> {
    ExactRatio::integer(timestamp.ticks).checked_mul(ExactRatio::new(
        i128::from(timestamp.time_base.numerator()),
        i128::from(timestamp.time_base.denominator()),
    )?)
}

fn rate_ratio(rate: FrameRate) -> Result<ExactRatio, TimeError> {
    ExactRatio::new(i128::from(rate.numerator()), i128::from(rate.denominator()))
}

fn video_span(snapshot: &SourceIndexSnapshot) -> Result<SourceSpan, ImportTimingError> {
    let index = snapshot.index();
    let last = index
        .frames()
        .last()
        .ok_or(ImportTimingError::UnmeasuredVideoEnd)?;
    if index.terminal_provenance() != TerminalProvenance::DecodedFrameDuration
        || last.reported_duration != index.terminal_end().checked_sub(last.pts)
    {
        return Err(ImportTimingError::UnmeasuredVideoEnd);
    }
    Ok(SourceSpan::new(
        SourceTimestamp {
            ticks: index.frames()[0].pts,
            time_base: index.time_base(),
        },
        SourceTimestamp {
            ticks: index.terminal_end(),
            time_base: index.time_base(),
        },
    )?)
}

fn audio_span(snapshot: &AudioIndexSnapshot) -> Result<SourceSpan, ImportTimingError> {
    let mut available = snapshot
        .frames()
        .iter()
        .filter(|frame| frame.valid_start < frame.valid_end);
    let first = available
        .next()
        .ok_or(ImportTimingError::UnavailableAudio)?;
    let mut end = first.valid_end;
    for frame in available {
        if frame.valid_start != end {
            return Err(ImportTimingError::UnavailableAudio);
        }
        end = frame.valid_end;
    }
    let time_base = SourceTimeBase::new(1, snapshot.stream().sample_rate)?;
    Ok(SourceSpan::new(
        SourceTimestamp {
            ticks: first.valid_start,
            time_base,
        },
        SourceTimestamp {
            ticks: end,
            time_base,
        },
    )?)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CadenceConfidence {
    /// Every adjacent presentation interval has the same exact length.
    ExactCfr,
    /// Repeated modal interval with at least 25% support; every observed
    /// interval is an integer multiple from one through eight of that mode.
    RepeatedIntegralVfr,
    /// One frame: only its positive decoded terminal duration supplies cadence.
    SingleDecodedFrame,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CadenceEvidence {
    pub confidence: CadenceConfidence,
    pub observed_intervals: u64,
    pub modal_intervals: u64,
    pub distinct_intervals: usize,
    pub tied_modes: usize,
    pub selected_interval_ticks: i64,
    pub observed_rate: FrameRate,
    pub presentation_divisor: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeometryEvidence {
    /// Exact display extent after SAR correction and clockwise quarter turns.
    pub display_width: ExactRatio,
    pub display_height: ExactRatio,
    pub rotation_quarter_turns: u8,
    pub sample_aspect_ratio: ExactRatio,
    /// Signed fractional difference of rounded and exact display aspect.
    pub relative_aspect_error: ExactRatio,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BasisCandidate {
    pub basis: PresentationBasis,
    pub cadence: CadenceEvidence,
    pub geometry: GeometryEvidence,
}

/// Display raster independently of cadence, for an explicitly previewed canvas
/// adoption after the project clock is already fixed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeometryCandidate {
    pub width: u32,
    pub height: u32,
    pub evidence: GeometryEvidence,
}

pub fn derive_presentation_geometry(
    snapshot: &SourceIndexSnapshot,
    info: &SourceStreamInfo,
) -> Result<GeometryCandidate, ImportTimingError> {
    if snapshot.stream_index() != info.stream_index
        || snapshot.index().time_base()
            != SourceTimeBase::new(info.time_base_num, info.time_base_den)?
    {
        return Err(ImportTimingError::VideoMetadataMismatch);
    }
    video_span(snapshot)?;
    let (width, height, evidence) = derive_geometry(info)?;
    Ok(GeometryCandidate {
        width,
        height,
        evidence,
    })
}

/// Derive a candidate only. This function does not choose which import is the
/// primary picture, change a document, or establish any basis-adoption state.
pub fn derive_presentation_basis(
    snapshot: &SourceIndexSnapshot,
    info: &SourceStreamInfo,
) -> Result<BasisCandidate, ImportTimingError> {
    let geometry = derive_presentation_geometry(snapshot, info)?;
    let (frame_rate, cadence) = derive_cadence(snapshot)?;
    Ok(BasisCandidate {
        basis: PresentationBasis {
            width: geometry.width,
            height: geometry.height,
            frame_rate,
            color_policy: color_policy(info),
        },
        cadence,
        geometry: geometry.evidence,
    })
}

/// The project color policy a qualified primary picture implies: its PQ or
/// HLG transfer selects the matching Rec.2020 HDR policy, anything else SDR.
pub fn color_policy(info: &SourceStreamInfo) -> ColorPolicy {
    match info.color.transfer {
        ColorTransfer::Pq => ColorPolicy::HdrRec2020Pq,
        ColorTransfer::Hlg => ColorPolicy::HdrRec2020Hlg,
        ColorTransfer::Bt709 | ColorTransfer::Srgb | ColorTransfer::Linear => {
            ColorPolicy::SdrRec709
        }
    }
}

/// The specification's provisional audio-only canvas. Adoption is host policy.
pub fn audio_only_basis() -> PresentationBasis {
    PresentationBasis {
        width: 1920,
        height: 1080,
        frame_rate: FrameRate::new(30, 1).expect("constant positive frame rate"),
        color_policy: ColorPolicy::SdrRec709,
    }
}

fn derive_cadence(
    snapshot: &SourceIndexSnapshot,
) -> Result<(FrameRate, CadenceEvidence), ImportTimingError> {
    let index = snapshot.index();
    let mut counts = BTreeMap::<i64, u64>::new();
    for pair in index.frames().windows(2) {
        let interval = pair[1]
            .pts
            .checked_sub(pair[0].pts)
            .ok_or(TimeError::Overflow)?;
        if !counts.contains_key(&interval) && counts.len() == MAX_CADENCE_INTERVALS {
            return Err(ImportTimingError::AmbiguousCadence);
        }
        *counts.entry(interval).or_default() += 1;
    }
    let single = counts.is_empty();
    if single {
        counts.insert(
            index.frames()[0]
                .reported_duration
                .ok_or(ImportTimingError::UnmeasuredVideoEnd)?,
            1,
        );
    }
    let mode_count = counts
        .values()
        .copied()
        .max()
        .ok_or(ImportTimingError::AmbiguousCadence)?;
    // BTreeMap order makes the first tied mode the shortest observed interval.
    let (&mode, _) = counts
        .iter()
        .find(|(_, count)| **count == mode_count)
        .ok_or(ImportTimingError::AmbiguousCadence)?;
    let total = counts.values().sum::<u64>();
    let confidence = if single {
        CadenceConfidence::SingleDecodedFrame
    } else if counts.len() == 1 {
        CadenceConfidence::ExactCfr
    } else {
        if mode_count < 2
            || mode_count * 4 < total
            || counts
                .keys()
                .any(|interval| interval % mode != 0 || interval / mode > 8)
        {
            return Err(ImportTimingError::AmbiguousCadence);
        }
        CadenceConfidence::RepeatedIntegralVfr
    };
    let interval_seconds = seconds(SourceTimestamp {
        ticks: mode,
        time_base: index.time_base(),
    })?;
    let observed = ExactRatio::ONE.checked_div(interval_seconds)?;
    let observed_rate = to_frame_rate(observed)?;
    let (rate, divisor) = cap_rate(observed)?;
    Ok((
        rate,
        CadenceEvidence {
            confidence,
            observed_intervals: total,
            modal_intervals: mode_count,
            distinct_intervals: counts.len(),
            tied_modes: counts
                .values()
                .filter(|count| **count == mode_count)
                .count(),
            selected_interval_ticks: mode,
            observed_rate,
            presentation_divisor: divisor,
        },
    ))
}

fn to_frame_rate(rate: ExactRatio) -> Result<FrameRate, ImportTimingError> {
    Ok(FrameRate::new(
        u32::try_from(rate.numerator()).map_err(|_| ImportTimingError::AmbiguousCadence)?,
        u32::try_from(rate.denominator()).map_err(|_| ImportTimingError::AmbiguousCadence)?,
    )?)
}

fn cap_rate(rate: ExactRatio) -> Result<(FrameRate, u32), ImportTimingError> {
    if !rate.compare_integer(60).is_gt() {
        return Ok((to_frame_rate(rate)?, 1));
    }
    // Highest common presentation rate that divides the observed cadence
    // exactly. Otherwise use its smallest integer divisor reaching <=60 fps.
    for (numerator, denominator) in [
        (60, 1),
        (60000, 1001),
        (50, 1),
        (48, 1),
        (48000, 1001),
        (30, 1),
        (30000, 1001),
        (25, 1),
        (24, 1),
        (24000, 1001),
    ] {
        let candidate = ExactRatio::new(numerator, denominator)?;
        let divisor = rate.checked_div(candidate)?;
        if divisor.denominator() == 1 {
            return Ok((
                to_frame_rate(candidate)?,
                u32::try_from(divisor.numerator())
                    .map_err(|_| ImportTimingError::AmbiguousCadence)?,
            ));
        }
    }
    let divisor = rate.checked_div(ExactRatio::integer(60))?.ceil()?;
    let reduced = rate.checked_div(ExactRatio::new(divisor, 1)?)?;
    Ok((
        to_frame_rate(reduced)?,
        u32::try_from(divisor).map_err(|_| ImportTimingError::AmbiguousCadence)?,
    ))
}

fn derive_geometry(
    info: &SourceStreamInfo,
) -> Result<(u32, u32, GeometryEvidence), ImportTimingError> {
    if !(1..=8192).contains(&info.width)
        || !(1..=8192).contains(&info.height)
        || info.sample_aspect_num == 0
        || info.sample_aspect_den == 0
        || info.rotation_quarter_turns > 3
    {
        return Err(ImportTimingError::UnsupportedGeometry);
    }
    let sar = ExactRatio::new(
        i128::from(info.sample_aspect_num),
        i128::from(info.sample_aspect_den),
    )?;
    let [_, _, clean_width, clean_height] = info
        .visible_bounds()
        .map_err(|_| ImportTimingError::UnsupportedGeometry)?;
    let mut display_width = clean_width.checked_mul(sar)?;
    let mut display_height = clean_height;
    if info.rotation_quarter_turns % 2 == 1 {
        std::mem::swap(&mut display_width, &mut display_height);
    }
    let width = nearest_even_dimension(display_width)?;
    let height = nearest_even_dimension(display_height)?;
    let exact_aspect = display_width.checked_div(display_height)?;
    let rounded_aspect = ExactRatio::new(i128::from(width), i128::from(height))?;
    let relative_aspect_error = rounded_aspect
        .checked_div(exact_aspect)?
        .checked_sub(ExactRatio::ONE)?;
    Ok((
        width,
        height,
        GeometryEvidence {
            display_width,
            display_height,
            rotation_quarter_turns: info.rotation_quarter_turns,
            sample_aspect_ratio: sar,
            relative_aspect_error,
        },
    ))
}

/// Closest even dimension, choosing down on a tie. This introduces no target
/// resolution enlargement; any SAR expansion corrects the original geometry.
/// Each axis may differ by at most one pixel from its exact display extent.
pub fn nearest_even_dimension(value: ExactRatio) -> Result<u32, ImportTimingError> {
    if value.compare_integer(1).is_lt() || value.compare_integer(65536).is_gt() {
        return Err(ImportTimingError::UnsupportedGeometry);
    }
    let half = value.checked_div(ExactRatio::integer(2))?;
    let lower = half.floor();
    let fraction = half.checked_sub(ExactRatio::new(lower, 1)?)?;
    let round_up = fraction
        .checked_mul(ExactRatio::integer(2))?
        .compare_integer(1)
        .is_gt();
    let dimension = ((lower + i128::from(round_up)) * 2).max(2);
    u32::try_from(dimension).map_err(|_| ImportTimingError::UnsupportedGeometry)
}
