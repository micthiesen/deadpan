//! An exact MP4 sample clock and explicit movie edit, without Matroska tick tolerance.
use super::{AudioChannelLayout, AudioFrameObservation, AudioIndexError, AudioStreamDescriptor};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mp4OpusFraming {
    pub channels: u32,
    pub pre_skip: u32,
    pub packet_count: u64,
    pub decoded_sample_count: u64,
    pub first_sample: i64,
    pub valid_samples: u64,
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
impl From<deadpan_source::audio::Mp4OpusFraming> for Mp4OpusFraming {
    fn from(value: deadpan_source::audio::Mp4OpusFraming) -> Self {
        Self {
            channels: value.channels,
            pre_skip: value.pre_skip,
            packet_count: value.packet_count,
            decoded_sample_count: value.decoded_sample_count,
            first_sample: value.first_sample,
            valid_samples: value.valid_samples,
        }
    }
}

fn require(value: bool, message: &'static str) -> Result<(), AudioIndexError> {
    if value {
        Ok(())
    } else {
        Err(AudioIndexError::Metadata(message))
    }
}

impl Mp4OpusFraming {
    pub(super) fn validate(
        self,
        stream: &AudioStreamDescriptor,
        frames: &[AudioFrameObservation],
    ) -> Result<(), AudioIndexError> {
        require(
            stream.codec == "opus"
                && stream.sample_rate == 48_000
                && stream.time_base.numerator() == 1
                && stream.time_base.denominator() == 48_000
                && stream.initial_padding == self.pre_skip
                && self.pre_skip <= u32::from(u16::MAX)
                && stream.seek_preroll == 3840
                && stream.trailing_padding == 0
                && matches!(
                    (self.channels, stream.channel_layout),
                    (
                        1,
                        AudioChannelLayout::Native {
                            channels: 1,
                            mask: 4
                        }
                    ) | (
                        2,
                        AudioChannelLayout::Native {
                            channels: 2,
                            mask: 3
                        }
                    )
                )
                && self.packet_count == frames.len() as u64
                && self.valid_samples > 0,
            "MP4 Opus stream/header contract",
        )?;
        let mut decoded = 0_u64;
        let mut duration = 0_u64;
        for (index, frame) in frames.iter().enumerate() {
            let count = u64::from(frame.sample_count);
            require(
                (120..=5760).contains(&count)
                    && count.is_multiple_of(120)
                    && i128::from(frame.pts) == i128::from(self.first_sample) + i128::from(decoded),
                "MP4 Opus packet clock or samples disagree with admission",
            )?;
            let ticks = frame.reported_duration.unwrap_or(0);
            require(
                ticks > 0
                    && ticks as u64 <= count
                    && (index + 1 == frames.len() || ticks as u64 == count),
                "MP4 Opus packet duration disagrees with physical samples",
            )?;
            let skip = frame.skip_samples;
            require(
                skip.map_or(0, |s| s.leading) == if index == 0 { self.pre_skip } else { 0 }
                    && skip.is_none_or(|s| {
                        s.trailing == 0 && s.leading_reason == 0 && s.trailing_reason == 0
                    })
                    && frame.discard == (decoded + count <= u64::from(self.pre_skip)),
                "MP4 Opus skip evidence disagrees with presentation edit",
            )?;
            decoded += count;
            duration += ticks as u64;
        }
        require(
            decoded == self.decoded_sample_count
                && duration.checked_sub(u64::from(self.pre_skip)) == Some(self.valid_samples),
            "MP4 Opus decoded inventory or presented endpoint differs from admission",
        )
    }
}
