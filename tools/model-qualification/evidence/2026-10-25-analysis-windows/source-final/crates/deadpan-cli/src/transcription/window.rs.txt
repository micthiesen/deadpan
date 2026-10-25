//! Exact-phase analysis windows on the complete Original's 16 kHz grid.

use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use deadpan_audio::{PcmWindow, ResampleRecipe, Resampler, StereoMatrix};
use deadpan_core::{AssetId, AudioSample, ExactRatio};
use deadpan_media::audio_index::AudioIndexSnapshot;
use deadpan_media::audio_session::{AudioSession, AudioSessionLimits, SourceAudioSample};
use deadpan_store::ProjectStore;
use deadpan_store::original_media::OriginalMediaLimits;
use deadpan_store::source_registration::SourceQualificationReceipt;

use super::{ANALYSIS_SAMPLE_RATE, AnalysisInput, OriginalAnalysis, TranscriptionError};

/// Local refinement prepares at most thirty seconds in one request.
pub const MAX_ANALYSIS_WINDOW_FRAMES: u64 = 30 * ANALYSIS_SAMPLE_RATE as u64;
const SOURCE_CHUNK_FRAMES: u32 = 32_768;

/// Selected PCM plus its exact position on the complete Original analysis grid.
///
/// Window sample `k` represents source sample
/// `analysis_origin + (analysis_offset + k) * source_rate / 16000`. Keep this
/// rational position intact: at 22.05/44.1 kHz the window's first source sample
/// usually is fractional. This deliberately is not an [`AnalysisInput`], whose
/// integer origin means the first sample of a complete analysis input.
#[derive(Debug, Clone)]
pub struct OriginalAnalysisWindow {
    pub samples: Vec<f32>,
    /// Source-sample origin of the complete Original analysis, never this crop.
    pub analysis_origin: i64,
    pub source_rate: u32,
    /// Exact 16 kHz sample offset from the complete analysis origin.
    pub analysis_offset: u64,
    /// The complete Original analysis extent, including qualified audio lead/tail.
    pub complete_analysis_frames: u64,
    /// Complete valid source-sample extent, independent of picture timing.
    pub source_range: Range<i64>,
    pub content: String,
    pub audio_stream: u32,
}

/// A reusable verified Original audio reader and its global resampling recipe.
///
/// Opening currently decodes the entire qualified stream into AudioSession's
/// private disk PCM cache. It is not bounded selected-range decoding. Only each
/// subsequent window's output allocation and source-read buffers are bounded;
/// retaining this session avoids rebuilding that cache for repeated windows.
pub struct OriginalAnalysisSession {
    session: AudioSession,
    resampler: Resampler,
    analysis_origin: i64,
    complete_analysis_frames: u64,
    source_range: Range<i64>,
    content: String,
    audio_stream: u32,
    source_rate: u32,
    channels: usize,
}

fn check(cancelled: &AtomicBool, deadline: Instant) -> Result<(), TranscriptionError> {
    if cancelled.load(Ordering::Acquire) {
        return Err(TranscriptionError::Cancelled);
    }
    if Instant::now() >= deadline {
        return Err(TranscriptionError::Deadline);
    }
    Ok(())
}

fn work<T>(
    result: Result<T, impl std::fmt::Display>,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<T, TranscriptionError> {
    check(cancelled, deadline)?;
    result.map_err(|error| TranscriptionError::Worker(error.to_string()))
}

fn remaining(cancelled: &AtomicBool, deadline: Instant) -> Result<Duration, TranscriptionError> {
    check(cancelled, deadline)?;
    Ok(deadline
        .saturating_duration_since(Instant::now())
        .min(Duration::from_secs(3_600)))
}

fn extent(index: &AudioIndexSnapshot) -> Result<(Range<i64>, u64), TranscriptionError> {
    let (Some(first), Some(last)) = (index.frames().first(), index.frames().last()) else {
        return Err(TranscriptionError::Configuration(
            "the Original audio is empty",
        ));
    };
    let source = first.valid_start..last.valid_end;
    let length = source
        .end
        .checked_sub(source.start)
        .filter(|length| *length > 0)
        .ok_or(TranscriptionError::Configuration(
            "the Original audio is empty",
        ))?;
    let frames = u64::try_from(
        i128::from(length) * i128::from(ANALYSIS_SAMPLE_RATE)
            / i128::from(index.stream().sample_rate),
    )
    .map_err(|_| TranscriptionError::Configuration("analysis length"))?;
    if frames == 0 || frames > deadpan_jobs::transcription::MAX_ANALYSIS_FRAMES {
        return Err(TranscriptionError::Configuration(
            "the Original audio is empty or longer than the analysis bound",
        ));
    }
    Ok((source, frames))
}

fn window_range(range: &Range<u64>, complete: u64) -> Result<(), TranscriptionError> {
    let length = range
        .end
        .checked_sub(range.start)
        .filter(|length| *length > 0)
        .ok_or(TranscriptionError::Configuration(
            "analysis window is empty or reversed",
        ))?;
    if length > MAX_ANALYSIS_WINDOW_FRAMES {
        return Err(TranscriptionError::Configuration(
            "analysis window exceeds thirty seconds",
        ));
    }
    if range.end > complete {
        return Err(TranscriptionError::Configuration(
            "analysis window is outside the Original",
        ));
    }
    Ok(())
}

/// Open a retained reader once for repeated exact-phase local windows.
/// Initial opening still builds the complete source PCM cache; see the type.
pub fn open_original_analysis(
    store: &ProjectStore,
    asset: Option<&AssetId>,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<OriginalAnalysisSession, TranscriptionError> {
    check(cancelled, deadline)?;
    let receipt = super::analysed_receipt(store, asset)?;
    OriginalAnalysisSession::open(store, receipt, cancelled, deadline)
}

/// Prepare a selected half-open range in complete-Original 16 kHz coordinates.
/// Empty, reversed, excessive and outside requests are refused using the
/// qualification receipt before source snapshotting, decoding or PCM allocation.
pub fn prepare_original_audio_window(
    store: &ProjectStore,
    asset: Option<&AssetId>,
    analysis_samples: Range<u64>,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<OriginalAnalysisWindow, TranscriptionError> {
    check(cancelled, deadline)?;
    // Static admission precedes even loading the receipt.
    window_range(&analysis_samples, u64::MAX)?;
    let receipt = super::analysed_receipt(store, asset)?;
    let expected = receipt
        .snapshot()
        .audio()
        .ok_or(TranscriptionError::Configuration(
            "the Original has no qualified audio",
        ))?;
    window_range(&analysis_samples, extent(expected)?.1)?;
    OriginalAnalysisSession::open(store, receipt, cancelled, deadline)?.prepare_window(
        analysis_samples,
        cancelled,
        deadline,
    )
}

impl OriginalAnalysisSession {
    fn open(
        store: &ProjectStore,
        receipt: SourceQualificationReceipt,
        cancelled: &AtomicBool,
        deadline: Instant,
    ) -> Result<Self, TranscriptionError> {
        check(cancelled, deadline)?;
        let snapshot = receipt.snapshot();
        let expected = snapshot.audio().ok_or(TranscriptionError::Configuration(
            "the Original has no qualified audio",
        ))?;
        let (source_range, complete_analysis_frames) = extent(expected)?;
        let stream = expected.stream();
        let channels = usize::try_from(stream.channel_layout.channels())
            .map_err(|_| TranscriptionError::Configuration("audio channels"))?;
        let bytes = expected
            .decoded_samples()
            .checked_mul(u64::from(stream.channel_layout.channels()))
            .and_then(|samples| samples.checked_mul(4))
            .ok_or(TranscriptionError::Configuration("Original audio size"))?;
        let mut limits = AudioSessionLimits {
            maximum_cache_bytes: bytes,
            maximum_index_frames: expected.frames().len(),
            opening_timeout: remaining(cancelled, deadline)?,
            ..AudioSessionLimits::default()
        };
        let matrix =
            work(
                StereoMatrix::new(snapshot.audio_layout().ok_or(
                    TranscriptionError::Configuration("the Original has no qualified audio"),
                )?),
                cancelled,
                deadline,
            )?;
        let original = work(
            store.snapshot_original(
                receipt.original().content(),
                work(
                    OriginalMediaLimits::new(
                        limits.decode.max_input_bytes,
                        remaining(cancelled, deadline)?,
                    ),
                    cancelled,
                    deadline,
                )?,
                cancelled,
            ),
            cancelled,
            deadline,
        )?;
        if original.record().object() != receipt.original() {
            return Err(TranscriptionError::Protocol(
                "retained Original differs from its receipt".into(),
            ));
        }
        let input = work(
            original.into_source_input_with_deadline(expected.content(), deadline, cancelled),
            cancelled,
            deadline,
        )?;
        limits.opening_timeout = remaining(cancelled, deadline)?;
        let session = work(
            AudioSession::open_input(input, stream.stream_index, limits, cancelled),
            cancelled,
            deadline,
        )?;
        if session.index() != expected {
            return Err(TranscriptionError::Protocol(
                "decoded Original differs from its audio receipt".into(),
            ));
        }
        let end = i64::try_from(complete_analysis_frames)
            .map_err(|_| TranscriptionError::Configuration("analysis length"))?;
        let recipe = work(
            ResampleRecipe::new(
                source_range.clone(),
                ExactRatio::integer(source_range.start),
                AudioSample(0),
                ExactRatio::new(
                    i128::from(stream.sample_rate),
                    i128::from(ANALYSIS_SAMPLE_RATE),
                )
                .map_err(|_| TranscriptionError::Configuration("source rate"))?,
                AudioSample(0)..AudioSample(end),
            ),
            cancelled,
            deadline,
        )?;
        Ok(Self {
            session,
            resampler: Resampler::new(recipe, matrix),
            analysis_origin: source_range.start,
            complete_analysis_frames,
            source_range,
            content: receipt.original().content().to_string(),
            audio_stream: stream.stream_index,
            source_rate: stream.sample_rate,
            channels,
        })
    }

    pub fn complete_analysis_frames(&self) -> u64 {
        self.complete_analysis_frames
    }

    /// Reuse this reader without rebuilding its full source cache. Output
    /// coordinates remain global, including the resampler's boundary halo.
    pub fn prepare_window(
        &self,
        analysis_samples: Range<u64>,
        cancelled: &AtomicBool,
        deadline: Instant,
    ) -> Result<OriginalAnalysisWindow, TranscriptionError> {
        check(cancelled, deadline)?;
        window_range(&analysis_samples, self.complete_analysis_frames)?;
        self.render(analysis_samples, cancelled, deadline)
    }

    fn render(
        &self,
        range: Range<u64>,
        cancelled: &AtomicBool,
        deadline: Instant,
    ) -> Result<OriginalAnalysisWindow, TranscriptionError> {
        let length = usize::try_from(range.end - range.start)
            .map_err(|_| TranscriptionError::Configuration("analysis length"))?;
        let mut samples = Vec::new();
        samples
            .try_reserve_exact(length)
            .map_err(|_| TranscriptionError::Configuration("analysis allocation"))?;
        let mut start = i64::try_from(range.start)
            .map_err(|_| TranscriptionError::Configuration("analysis offset"))?;
        let end = i64::try_from(range.end)
            .map_err(|_| TranscriptionError::Configuration("analysis endpoint"))?;
        let mut buffer_start = self.source_range.start;
        let mut buffer = Vec::new();
        while start < end {
            check(cancelled, deadline)?;
            let count =
                u32::try_from((end - start).min(i64::from(deadpan_audio::MAX_OUTPUT_FRAMES)))
                    .map_err(|_| TranscriptionError::Configuration("analysis block"))?;
            let required = work(
                self.resampler
                    .required_source_range(AudioSample(start), count),
                cancelled,
                deadline,
            )?;
            let window = match required {
                None => None,
                Some(required) => {
                    let buffered = i64::try_from(buffer.len() / self.channels)
                        .map_err(|_| TranscriptionError::Configuration("source buffer"))?;
                    let buffer_end = buffer_start
                        .checked_add(buffered)
                        .ok_or(TranscriptionError::Configuration("source buffer endpoint"))?;
                    if required.start < buffer_start || required.end > buffer_end {
                        let length = (self.source_range.end - required.start)
                            .min(i64::from(SOURCE_CHUNK_FRAMES))
                            .max(required.end - required.start);
                        let length = u32::try_from(length)
                            .map_err(|_| TranscriptionError::Configuration("source read size"))?;
                        if length > SOURCE_CHUNK_FRAMES {
                            return Err(TranscriptionError::Configuration(
                                "source halo exceeds read bound",
                            ));
                        }
                        let read = work(
                            self.session.read_samples(
                                SourceAudioSample(required.start),
                                length,
                                remaining(cancelled, deadline)?.min(Duration::from_secs(30)),
                                cancelled,
                            ),
                            cancelled,
                            deadline,
                        )?;
                        buffer_start = read.start.0;
                        buffer = read.samples;
                    }
                    let offset = usize::try_from(required.start - buffer_start)
                        .ok()
                        .and_then(|n| n.checked_mul(self.channels))
                        .ok_or(TranscriptionError::Configuration("source buffer offset"))?;
                    let length = usize::try_from(required.end - required.start)
                        .ok()
                        .and_then(|n| n.checked_mul(self.channels))
                        .ok_or(TranscriptionError::Configuration("source buffer length"))?;
                    let last = offset
                        .checked_add(length)
                        .ok_or(TranscriptionError::Configuration("source buffer endpoint"))?;
                    let selected = buffer
                        .get(offset..last)
                        .ok_or(TranscriptionError::Configuration("source halo unavailable"))?;
                    Some(PcmWindow {
                        start: required.start,
                        samples: selected.to_vec(),
                    })
                }
            };
            let rendered = work(
                self.resampler
                    .render(AudioSample(start), count, window, cancelled),
                cancelled,
                deadline,
            )?;
            samples.extend(
                rendered
                    .samples
                    .iter()
                    .map(|[left, right]| (left + right) * 0.5),
            );
            start += i64::from(count);
        }
        check(cancelled, deadline)?;
        Ok(OriginalAnalysisWindow {
            samples,
            analysis_origin: self.analysis_origin,
            source_rate: self.source_rate,
            analysis_offset: range.start,
            complete_analysis_frames: self.complete_analysis_frames,
            source_range: self.source_range.clone(),
            content: self.content.clone(),
            audio_stream: self.audio_stream,
        })
    }
}

pub(super) fn prepare_full(
    store: &ProjectStore,
    asset: Option<&AssetId>,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<OriginalAnalysis, TranscriptionError> {
    let session = open_original_analysis(store, asset, cancelled, deadline)?;
    let window = session.render(0..session.complete_analysis_frames, cancelled, deadline)?;
    Ok(OriginalAnalysis {
        input: AnalysisInput {
            samples: window.samples,
            origin: window.analysis_origin,
            source_rate: window.source_rate,
        },
        content: window.content,
        audio_stream: window.audio_stream,
    })
}
