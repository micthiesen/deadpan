//! Exact Opus sample positions checked against every retained Matroska timestamp.
//! The codec sample clock supplies spacing; a container tick is a quantization
//! envelope, never cumulative drift, resampling, inserted silence or event alignment.

use super::{AudioChannelLayout, AudioFrameObservation, AudioIndexError, AudioStreamDescriptor};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MatroskaOpusClock {
    pub pre_skip: u32,
    pub codec_delay_ns: u64,
    pub timestamp_scale_ns: u64,
    pub first_block_timestamp: i64,
    pub packet_count: u64,
    pub decoded_sample_count: u64,
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
impl From<deadpan_source::audio::MatroskaOpusClock> for MatroskaOpusClock {
    fn from(value: deadpan_source::audio::MatroskaOpusClock) -> Self {
        Self {
            pre_skip: value.pre_skip,
            codec_delay_ns: value.codec_delay_ns,
            timestamp_scale_ns: value.timestamp_scale_ns,
            first_block_timestamp: value.first_block_timestamp,
            packet_count: value.packet_count,
            decoded_sample_count: value.decoded_sample_count,
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

pub(super) struct Timeline {
    clock: MatroskaOpusClock,
    origin: i64,
    delay_ticks: i128,
}

impl MatroskaOpusClock {
    pub(super) fn validate(
        self,
        stream: &AudioStreamDescriptor,
    ) -> Result<Timeline, AudioIndexError> {
        require(
            stream.codec == "opus"
                && stream.sample_rate == 48_000
                && self.pre_skip <= u32::from(u16::MAX)
                && stream.initial_padding == self.pre_skip
                && stream.trailing_padding == 0
                && stream.seek_preroll == 3840
                && matches!(
                    stream.channel_layout,
                    AudioChannelLayout::Native {
                        channels: 1,
                        mask: 4
                    } | AudioChannelLayout::Native {
                        channels: 2,
                        mask: 3
                    }
                ),
            "Opus stream/header contract",
        )?;
        require(
            (1..=1_000_000).contains(&self.timestamp_scale_ns)
                && u128::from(stream.time_base.numerator()) * 1_000_000_000
                    == u128::from(stream.time_base.denominator())
                        * u128::from(self.timestamp_scale_ns),
            "Opus container clock disagrees with admitted timestamp scale",
        )?;
        require(
            (i128::from(self.codec_delay_ns) * 48_000 - i128::from(self.pre_skip) * 1_000_000_000)
                .abs()
                < 48_000,
            "Opus CodecDelay disagrees with exact pre-skip",
        )?;
        let tick = i128::from(self.timestamp_scale_ns);
        let first = i128::from(self.first_block_timestamp) * tick * 48_000;
        require(
            first % 1_000_000_000 == 0,
            "Opus initial block does not name an exact sample",
        )?;
        let origin = i64::try_from(first / 1_000_000_000 - i128::from(self.pre_skip))
            .map_err(|_| AudioIndexError::Metadata("Opus sample coordinate overflow"))?;
        Ok(Timeline {
            clock: self,
            origin,
            delay_ticks: (i128::from(self.codec_delay_ns) + tick / 2) / tick,
        })
    }
}

impl Timeline {
    pub(super) fn position(
        &self,
        frame: &AudioFrameObservation,
        decoded: u64,
    ) -> Result<i64, AudioIndexError> {
        require(
            (120..=5760).contains(&frame.sample_count) && frame.sample_count.is_multiple_of(120),
            "Opus packet sample count",
        )?;
        let block = i128::from(frame.pts) + self.delay_ticks;
        let ticks = block - i128::from(self.clock.first_block_timestamp);
        let observed = ticks * i128::from(self.clock.timestamp_scale_ns) * 48_000;
        let exact = i128::from(decoded) * 1_000_000_000;
        require(
            (decoded != 0 || ticks == 0)
                && (observed - exact).abs() <= i128::from(self.clock.timestamp_scale_ns) * 48_000,
            "Opus packet clock exceeds one container tick of exact sample continuity",
        )?;
        i64::try_from(i128::from(self.origin) + i128::from(decoded))
            .map_err(|_| AudioIndexError::Metadata("Opus sample coordinate overflow"))
    }

    pub(super) fn duration(
        &self,
        frame: &AudioFrameObservation,
        ticks: i64,
    ) -> Result<(), AudioIndexError> {
        let actual = i128::from(ticks) * i128::from(self.clock.timestamp_scale_ns) * 48_000;
        let physical = i128::from(frame.sample_count) * 1_000_000_000;
        let tail = i128::from(frame.skip_samples.map_or(0, |skip| skip.trailing)) * 1_000_000_000;
        let tolerance = i128::from(self.clock.timestamp_scale_ns) * 48_000;
        require(
            (actual - physical).abs() <= tolerance
                || (actual - (physical - tail)).abs() <= tolerance,
            "Opus reported duration disagrees with physical samples and explicit trim",
        )
    }
}
