//! Exact raw MP3 sample positions and per-frame evidence for declared trim.
use super::{AudioChannelLayout, AudioFrameObservation, AudioIndexError, AudioStreamDescriptor};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mp3Framing {
    pub sample_rate: u32,
    pub channels: u32,
    pub samples_per_frame: u32,
    pub frame_count: u64,
    pub leading_skip: u32,
    pub trailing_skip: u32,
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
impl From<deadpan_source::audio::Mp3Framing> for Mp3Framing {
    fn from(value: deadpan_source::audio::Mp3Framing) -> Self {
        Self {
            sample_rate: value.sample_rate,
            channels: value.channels,
            samples_per_frame: value.samples_per_frame,
            frame_count: value.frame_count,
            leading_skip: value.leading_skip,
            trailing_skip: value.trailing_skip,
        }
    }
}

impl Mp3Framing {
    pub(super) fn validate(
        self,
        stream: &AudioStreamDescriptor,
        frames: usize,
    ) -> Result<(), AudioIndexError> {
        let samples = match self.sample_rate {
            32_000 | 44_100 | 48_000 => 1152,
            8_000 | 11_025 | 12_000 | 16_000 | 22_050 | 24_000 => 576,
            _ => return Err(AudioIndexError::Metadata("MP3 sample rate")),
        };
        if stream.codec != "mp3"
            || stream.stream_index != 0
            || stream.sample_rate != self.sample_rate
            || stream.time_base.numerator() != 1
            || stream.time_base.denominator() != 14_112_000
            || stream.initial_padding != 0
            || stream.trailing_padding != 0
            || stream.seek_preroll != 0
            || stream.stream_start
                != Some(i64::from(self.leading_skip) * 14_112_000 / i64::from(self.sample_rate))
            || self.channels != stream.channel_layout.channels()
            || !matches!(
                stream.channel_layout,
                AudioChannelLayout::Native {
                    channels: 1,
                    mask: 4
                } | AudioChannelLayout::Native {
                    channels: 2,
                    mask: 3
                }
            )
            || self.samples_per_frame != samples
            || self.frame_count != frames as u64
            || frames < 2
            || !(self.leading_skip == 0 && self.trailing_skip == 0
                || (529..=4624).contains(&self.leading_skip) && self.trailing_skip <= 3566)
            || u64::from(self.leading_skip) + u64::from(self.trailing_skip)
                >= self.frame_count * u64::from(samples)
        {
            return Err(AudioIndexError::Metadata("MP3 stream and frame inventory"));
        }
        Ok(())
    }

    pub(super) fn trim(
        self,
        frame: &AudioFrameObservation,
        ordinal: usize,
        decoded: u64,
    ) -> Result<(i64, i64), AudioIndexError> {
        let count = u64::from(self.samples_per_frame);
        let first_padding = if ordinal == 0 { self.leading_skip } else { 0 };
        let audible_end = self.frame_count * count - u64::from(self.trailing_skip);
        let trailing = (decoded + count).saturating_sub(audible_end).min(count);
        let skip = frame.skip_samples;
        if frame.discard
            || frame.sample_count != self.samples_per_frame
            || i128::from(frame.pts) * i128::from(self.sample_rate)
                != i128::from(decoded) * 14_112_000
            || frame
                .reported_duration
                .map(i128::from)
                .map(|n| n * i128::from(self.sample_rate))
                != Some(i128::from(count) * 14_112_000)
            || skip.map_or(0, |s| s.leading) != first_padding
            || u64::from(skip.map_or(0, |s| s.trailing)) != trailing
            || skip.is_some_and(|s| s.leading_reason != 0 || s.trailing_reason != 0)
        {
            return Err(AudioIndexError::Metadata(
                "MP3 frame clock or skip evidence disagrees with admitted framing",
            ));
        }
        let leading = u64::from(self.leading_skip)
            .saturating_sub(decoded)
            .min(count);
        Ok((leading as i64, trailing as i64))
    }
}
