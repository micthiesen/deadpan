//! Measured original audio backed by a bounded, disposable private PCM cache.
//! Opening and reading belong on media threads, never a real-time callback.

use std::fs::File;
use std::io::{Read, Write};
use std::os::unix::fs::FileExt;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use deadpan_core::SourceTimeBase;
use deadpan_source::audio::{AudioDecodeLimits, AudioDecoder, AudioSampleFormat};
use deadpan_source::{DecodeControl, SourceDecodeError};

use crate::ConversionError;
use crate::audio_index::{
    AudioChannelLayout, AudioFrameObservation, AudioIndexError, AudioIndexSnapshot,
    AudioSkipSamples, AudioStreamDescriptor, MAX_AUDIO_INDEX_FRAMES,
};
use crate::conversion::Deadline;
use crate::source_index::SourceContentIdentity;
use crate::source_input::VerifiedSourceInput;

#[derive(Debug, Clone, Copy)]
pub struct AudioSessionLimits {
    pub decode: AudioDecodeLimits,
    pub maximum_index_frames: usize,
    /// Physical interleaved f32 bytes, including retained priming and padding.
    pub maximum_cache_bytes: u64,
    pub maximum_read_frames: u32,
    /// One deadline covers copying, opening, decoding, indexing and cache writes.
    pub opening_timeout: Duration,
}

impl Default for AudioSessionLimits {
    fn default() -> Self {
        Self {
            decode: AudioDecodeLimits::default(),
            maximum_index_frames: MAX_AUDIO_INDEX_FRAMES,
            maximum_cache_bytes: 1024 * 1024 * 1024,
            maximum_read_frames: 65_536,
            opening_timeout: Duration::from_secs(300),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AudioSessionError {
    #[error("invalid audio session limits or exhausted budget: {0}")]
    Limits(&'static str),
    #[error("requested original audio range includes unavailable or excluded samples")]
    UnavailableRange,
    #[error("decoded audio changed its stream contract")]
    IndexMismatch,
    #[error(transparent)]
    Snapshot(#[from] ConversionError),
    #[error(transparent)]
    Native(#[from] SourceDecodeError),
    #[error(transparent)]
    Index(#[from] AudioIndexError),
    #[error(transparent)]
    Time(#[from] deadpan_core::TimeError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Original sample frames, independent of the project's 48 kHz mix clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceAudioSample(pub i64);

#[derive(Debug, Clone, PartialEq)]
pub struct OriginalAudioBlock {
    pub start: SourceAudioSample,
    pub sample_rate: u32,
    pub channel_layout: AudioChannelLayout,
    pub samples: Vec<f32>,
}

pub struct AudioSession {
    cache: File,
    index: AudioIndexSnapshot,
    maximum_read_frames: u32,
}

impl AudioSession {
    pub fn open_verified(
        source: &mut impl Read,
        identity: SourceContentIdentity,
        selected_stream: u32,
        limits: AudioSessionLimits,
        cancelled: &AtomicBool,
    ) -> Result<Self, AudioSessionError> {
        Self::validate_limits(identity, Some(selected_stream), limits)?;
        let deadline = Deadline {
            end: Instant::now() + limits.opening_timeout,
            cancelled,
        };
        let input = VerifiedSourceInput::copy_with_deadline(source, identity, &deadline)?;
        Self::open_with_deadline(input, Some(selected_stream), limits, &deadline)
    }

    /// Reuses already verified source bytes. The opening deadline starts here;
    /// creating that shared snapshot was separately bounded.
    pub fn open_input(
        input: VerifiedSourceInput,
        selected_stream: u32,
        limits: AudioSessionLimits,
        cancelled: &AtomicBool,
    ) -> Result<Self, AudioSessionError> {
        Self::open_selected_input(input, Some(selected_stream), limits, cancelled)
    }

    /// Select the actual first audio stream during one guarded container open.
    /// The measured index retains its actual absolute stream index.
    pub fn open_first_input(
        input: VerifiedSourceInput,
        limits: AudioSessionLimits,
        cancelled: &AtomicBool,
    ) -> Result<Self, AudioSessionError> {
        Self::open_selected_input(input, None, limits, cancelled)
    }

    fn open_selected_input(
        input: VerifiedSourceInput,
        selected_stream: Option<u32>,
        limits: AudioSessionLimits,
        cancelled: &AtomicBool,
    ) -> Result<Self, AudioSessionError> {
        Self::validate_limits(input.identity(), selected_stream, limits)?;
        let deadline = Deadline {
            end: Instant::now() + limits.opening_timeout,
            cancelled,
        };
        Self::open_with_deadline(input, selected_stream, limits, &deadline)
    }

    fn validate_limits(
        identity: SourceContentIdentity,
        stream: Option<u32>,
        limits: AudioSessionLimits,
    ) -> Result<(), AudioSessionError> {
        limits.decode.validate()?;
        if identity.byte_length() > limits.decode.max_input_bytes
            || stream.is_some_and(|index| index >= 33)
            || !(1..=MAX_AUDIO_INDEX_FRAMES).contains(&limits.maximum_index_frames)
            || !(1..=16 * 1024 * 1024 * 1024).contains(&limits.maximum_cache_bytes)
            || !(1..=65_536).contains(&limits.maximum_read_frames)
            || limits.opening_timeout.is_zero()
            || limits.opening_timeout > Duration::from_secs(86400)
        {
            return Err(AudioSessionError::Limits(
                "invalid input, stream, time, byte or frame budget",
            ));
        }
        Ok(())
    }

    fn open_with_deadline(
        input: VerifiedSourceInput,
        selected_stream: Option<u32>,
        limits: AudioSessionLimits,
        deadline: &Deadline<'_>,
    ) -> Result<Self, AudioSessionError> {
        let mut decoder = match selected_stream {
            Some(stream) => AudioDecoder::open(
                input.decoder_file()?,
                stream,
                limits.decode,
                control(deadline)?,
            )?,
            None => {
                AudioDecoder::open_first(input.decoder_file()?, limits.decode, control(deadline)?)?
            }
        };
        let info = decoder.info().clone();
        let stream = AudioStreamDescriptor {
            stream_index: info.stream_index,
            codec: info.codec.clone(),
            time_base: SourceTimeBase::new(info.time_base_num, info.time_base_den)?,
            sample_rate: info.sample_rate,
            channel_layout: layout(info.channel_layout),
            stream_start: info.stream_start,
            stream_duration: info.stream_duration,
            initial_padding: info.initial_padding,
            trailing_padding: info.trailing_padding,
            seek_preroll: info.seek_preroll,
        };
        let mut cache = tempfile::tempfile()?;
        let mut observations = Vec::new();
        let mut cache_bytes = 0_u64;
        while let Some(frame) = decoder.next_metadata(control(deadline)?)? {
            if frame.sample_rate != info.sample_rate
                || frame.channel_layout != info.channel_layout
                || frame.sample_format != info.sample_format
            {
                return Err(AudioSessionError::IndexMismatch);
            }
            if observations.len() >= limits.maximum_index_frames {
                return Err(AudioSessionError::Limits("audio index frame budget"));
            }
            let length =
                u64::from(frame.nb_samples) * u64::from(info.channel_layout.channels()) * 4;
            cache_bytes = cache_bytes
                .checked_add(length)
                .filter(|bytes| *bytes <= limits.maximum_cache_bytes)
                .ok_or(AudioSessionError::Limits("PCM cache byte budget"))?;
            if observations.len() == observations.capacity() {
                observations
                    .try_reserve_exact((limits.maximum_index_frames - observations.len()).min(1024))
                    .map_err(|_| AudioSessionError::Limits("audio index allocation"))?;
            }
            let decoded = decoder.copy_current_interleaved_f32(control(deadline)?)?;
            if decoded.metadata != frame || decoded.samples.len() as u64 * 4 != length {
                return Err(AudioSessionError::IndexMismatch);
            }
            let mut buffer = [0_u8; 16 * 1024];
            for chunk in decoded.samples.chunks(buffer.len() / 4) {
                deadline.check()?;
                for (value, bytes) in chunk.iter().zip(buffer.chunks_exact_mut(4)) {
                    if !value.is_finite() {
                        return Err(AudioSessionError::IndexMismatch);
                    }
                    bytes.copy_from_slice(&value.to_le_bytes());
                }
                cache.write_all(&buffer[..chunk.len() * 4])?;
            }
            observations.push(AudioFrameObservation {
                pts: frame.pts,
                discard: frame.discard,
                decode_timestamp: frame.decode_timestamp,
                reported_duration: frame.reported_duration,
                sample_count: frame.nb_samples,
                sample_format: match frame.sample_format {
                    AudioSampleFormat::Signed16 => "s16",
                    AudioSampleFormat::Float32Planar => "fltp",
                }
                .into(),
                skip_samples: frame.skip_samples.map(|skip| AudioSkipSamples {
                    leading: skip.leading,
                    trailing: skip.trailing,
                    leading_reason: skip.leading_reason,
                    trailing_reason: skip.trailing_reason,
                }),
            });
        }
        deadline.check()?;
        let index =
            AudioIndexSnapshot::new_controlled(input.identity(), stream, observations, || {
                deadline.check().map_err(AudioSessionError::from)
            })?;
        deadline.check()?;
        if cache.metadata()?.len() != cache_bytes {
            return Err(AudioSessionError::IndexMismatch);
        }
        Ok(Self {
            cache,
            index,
            maximum_read_frames: limits.maximum_read_frames,
        })
    }

    pub fn index(&self) -> &AudioIndexSnapshot {
        &self.index
    }

    /// Returns only measured valid coverage. Padding, unknown ranges and gaps
    /// are errors; this layer does not fill silence or choose an editorial origin.
    pub fn read_samples(
        &self,
        start: SourceAudioSample,
        frames: u32,
        timeout: Duration,
        cancelled: &AtomicBool,
    ) -> Result<OriginalAudioBlock, AudioSessionError> {
        if frames == 0
            || frames > self.maximum_read_frames
            || timeout.is_zero()
            || timeout > Duration::from_secs(60)
        {
            return Err(AudioSessionError::Limits("audio read frame or time budget"));
        }
        let deadline = Deadline {
            end: Instant::now() + timeout,
            cancelled,
        };
        deadline.check()?;
        let end = start
            .0
            .checked_add(i64::from(frames))
            .ok_or(AudioSessionError::UnavailableRange)?;
        let first = self
            .index
            .frames()
            .partition_point(|frame| frame.valid_end <= start.0);
        // Validate complete coverage before allocating or returning any samples.
        self.visit_range(first, start.0, end, &deadline, |_, _| Ok(()))?;
        let channels = self.index.stream().channel_layout.channels() as usize;
        let count = (frames as usize)
            .checked_mul(channels)
            .ok_or(AudioSessionError::Limits("audio read size"))?;
        let mut samples = Vec::new();
        samples
            .try_reserve_exact(count)
            .map_err(|_| AudioSessionError::Limits("audio read allocation"))?;
        let mut buffer = [0_u8; 16 * 1024];
        self.visit_range(
            first,
            start.0,
            end,
            &deadline,
            |cache_start, frame_count| {
                let mut offset = cache_start * channels as u64 * 4;
                let mut remaining = frame_count * channels as u64 * 4;
                while remaining != 0 {
                    deadline.check()?;
                    let length = remaining.min(buffer.len() as u64) as usize;
                    self.cache.read_exact_at(&mut buffer[..length], offset)?;
                    samples.extend(
                        buffer[..length].chunks_exact(4).map(|bytes| {
                            f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
                        }),
                    );
                    remaining -= length as u64;
                    offset += length as u64;
                }
                Ok(())
            },
        )?;
        deadline.check()?;
        Ok(OriginalAudioBlock {
            start,
            sample_rate: self.index.stream().sample_rate,
            channel_layout: self.index.stream().channel_layout,
            samples,
        })
    }

    fn visit_range(
        &self,
        first: usize,
        mut position: i64,
        end: i64,
        deadline: &Deadline<'_>,
        mut visit: impl FnMut(u64, u64) -> Result<(), AudioSessionError>,
    ) -> Result<(), AudioSessionError> {
        for frame in &self.index.frames()[first..] {
            deadline.check()?;
            if frame.valid_start == frame.valid_end {
                continue;
            }
            if position < frame.valid_start || position >= frame.valid_end {
                return Err(AudioSessionError::UnavailableRange);
            }
            let next = frame.valid_end.min(end);
            let cache_start = frame.cache_start + (position - frame.source_start) as u64;
            visit(cache_start, (next - position) as u64)?;
            position = next;
            if position == end {
                return Ok(());
            }
        }
        deadline.check()?;
        Err(AudioSessionError::UnavailableRange)
    }
}

fn layout(layout: deadpan_source::audio::AudioChannelLayout) -> AudioChannelLayout {
    match layout {
        deadpan_source::audio::AudioChannelLayout::Unspecified { channels } => {
            AudioChannelLayout::Unspecified { channels }
        }
        deadpan_source::audio::AudioChannelLayout::Native { channels, mask } => {
            AudioChannelLayout::Native { channels, mask }
        }
    }
}

fn control<'a>(deadline: &Deadline<'a>) -> Result<DecodeControl<'a>, AudioSessionError> {
    deadline.check()?;
    Ok(DecodeControl {
        timeout: deadline
            .end
            .saturating_duration_since(Instant::now())
            .min(Duration::from_secs(60)),
        cancelled: deadline.cancelled,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    #[test]
    fn range_scan_checks_cancellation_between_segments_and_preserves_deadline_errors() {
        let stream = AudioStreamDescriptor {
            stream_index: 0,
            codec: "pcm_s16le".into(),
            time_base: SourceTimeBase::new(1, 48000).unwrap(),
            sample_rate: 48000,
            channel_layout: AudioChannelLayout::Unspecified { channels: 1 },
            stream_start: None,
            stream_duration: None,
            initial_padding: 0,
            trailing_padding: 0,
            seek_preroll: 0,
        };
        let observations = [0, 4, 8]
            .into_iter()
            .map(|pts| AudioFrameObservation {
                pts,
                discard: false,
                decode_timestamp: None,
                reported_duration: Some(4),
                sample_count: 4,
                sample_format: "s16".into(),
                skip_samples: None,
            })
            .collect();
        let session = AudioSession {
            cache: tempfile::tempfile().unwrap(),
            index: AudioIndexSnapshot::new(
                SourceContentIdentity::new([4; 32], 100).unwrap(),
                stream,
                observations,
            )
            .unwrap(),
            maximum_read_frames: 65536,
        };
        let cancelled = AtomicBool::new(false);
        let deadline = Deadline {
            end: Instant::now() + Duration::from_secs(1),
            cancelled: &cancelled,
        };
        let mut calls = 0;
        let result = session.visit_range(0, 0, 12, &deadline, |_, _| {
            calls += 1;
            cancelled.store(true, Ordering::Release);
            Ok(())
        });
        assert!(matches!(
            result,
            Err(AudioSessionError::Snapshot(ConversionError::Cancelled))
        ));
        assert_eq!(calls, 1);
        cancelled.store(false, Ordering::Release);
        let expired = Deadline {
            end: Instant::now() - Duration::from_secs(1),
            cancelled: &cancelled,
        };
        assert!(matches!(
            session.visit_range(0, -1, 1, &expired, |_, _| panic!(
                "expired range must not read"
            )),
            Err(AudioSessionError::Snapshot(ConversionError::Deadline))
        ));
    }
}
