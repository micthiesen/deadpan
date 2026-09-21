//! Original audio clocks and measured decode coverage. Container claims remain
//! observations; neither stream duration nor nominal video rate sets an endpoint.

use deadpan_core::SourceTimeBase;
use serde::{Deserialize, Serialize};

use crate::source_index::{MAX_SOURCE_INDEX_JSON_BYTES, SourceContentIdentity};

pub const AUDIO_INDEX_VERSION: u32 = 1;
pub const AUDIO_DECODER_CONTRACT: &str = "ffmpeg-8.0.3/audio-manual-skip-v1";
pub const MAX_AUDIO_INDEX_FRAMES: usize = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "order", rename_all = "snake_case", deny_unknown_fields)]
pub enum AudioChannelLayout {
    Unspecified { channels: u32 },
    Native { channels: u32, mask: u64 },
}

impl AudioChannelLayout {
    pub fn channels(self) -> u32 {
        match self {
            Self::Unspecified { channels } | Self::Native { channels, .. } => channels,
        }
    }

    fn validate(self) -> Result<(), AudioIndexError> {
        if !(1..=32).contains(&self.channels())
            || matches!(self, Self::Native { channels, mask } if mask.count_ones() != channels)
        {
            return Err(AudioIndexError::Metadata("unsupported channel layout"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioStreamDescriptor {
    pub stream_index: u32,
    pub codec: String,
    pub time_base: SourceTimeBase,
    pub sample_rate: u32,
    pub channel_layout: AudioChannelLayout,
    pub stream_start: Option<i64>,
    pub stream_duration: Option<i64>,
    pub initial_padding: u32,
    pub trailing_padding: u32,
    pub seek_preroll: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioSkipSamples {
    pub leading: u32,
    pub trailing: u32,
    pub leading_reason: u8,
    pub trailing_reason: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioFrameObservation {
    pub pts: i64,
    pub discard: bool,
    pub decode_timestamp: Option<i64>,
    pub reported_duration: Option<i64>,
    pub sample_count: u32,
    pub sample_format: String,
    pub skip_samples: Option<AudioSkipSamples>,
}

/// Positions count original sample frames, before any project resampling.
/// `cache_start` counts physically decoded frames, including excluded padding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexedAudioFrame {
    pub source_start: i64,
    pub valid_start: i64,
    pub valid_end: i64,
    pub cache_start: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum AudioIndexError {
    #[error("unsupported audio index schema or decoder contract")]
    Schema,
    #[error("audio index exceeds its byte or frame budget")]
    Limit,
    #[error("invalid or unsupported exact audio metadata: {0}")]
    Metadata(&'static str),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// A checked cache envelope, not proof that the identified original is present.
/// Derived offsets are reconstructed from raw observations when deserialized.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "AudioIndexWire")]
pub struct AudioIndexSnapshot {
    schema_version: u32,
    decoder_contract: String,
    content: SourceContentIdentity,
    stream: AudioStreamDescriptor,
    observations: Vec<AudioFrameObservation>,
    #[serde(skip)]
    frames: Vec<IndexedAudioFrame>,
    #[serde(skip)]
    decoded_samples: u64,
    #[serde(skip)]
    valid_samples: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AudioIndexWire {
    schema_version: u32,
    decoder_contract: String,
    content: SourceContentIdentity,
    stream: AudioStreamDescriptor,
    observations: Vec<AudioFrameObservation>,
}

impl TryFrom<AudioIndexWire> for AudioIndexSnapshot {
    type Error = AudioIndexError;
    fn try_from(value: AudioIndexWire) -> Result<Self, Self::Error> {
        if value.schema_version != AUDIO_INDEX_VERSION
            || value.decoder_contract != AUDIO_DECODER_CONTRACT
        {
            return Err(AudioIndexError::Schema);
        }
        Self::new(value.content, value.stream, value.observations)
    }
}

impl AudioIndexSnapshot {
    pub fn new(
        content: SourceContentIdentity,
        stream: AudioStreamDescriptor,
        observations: Vec<AudioFrameObservation>,
    ) -> Result<Self, AudioIndexError> {
        Self::new_controlled(content, stream, observations, || Ok(()))
    }

    /// Media services supply their existing deadline/cancellation check. Keep
    /// its error type intact while sharing the same pure index validation.
    pub(crate) fn new_controlled<E: From<AudioIndexError>>(
        content: SourceContentIdentity,
        stream: AudioStreamDescriptor,
        observations: Vec<AudioFrameObservation>,
        mut check: impl FnMut() -> Result<(), E>,
    ) -> Result<Self, E> {
        check()?;
        stream.channel_layout.validate()?;
        if stream.stream_index >= 33
            || !(1..=384_000).contains(&stream.sample_rate)
            || !matches!(stream.codec.as_str(), "aac" | "pcm_s16le")
        {
            return Err(AudioIndexError::Metadata("stream contract").into());
        }
        if observations.is_empty() || observations.len() > MAX_AUDIO_INDEX_FRAMES {
            return Err(AudioIndexError::Limit.into());
        }
        let mut frames = Vec::new();
        frames
            .try_reserve_exact(observations.len())
            .map_err(|_| AudioIndexError::Limit)?;
        let mut decoded_samples = 0_u64;
        let mut valid_samples = 0_u64;
        let mut previous_end = None;
        for observation in &observations {
            check()?;
            if observation.sample_count == 0
                || observation.sample_count > 65_536
                || !matches!(
                    (stream.codec.as_str(), observation.sample_format.as_str()),
                    ("pcm_s16le", "s16") | ("aac", "fltp")
                )
            {
                return Err(AudioIndexError::Metadata("frame samples or format").into());
            }
            let start = sample_position(observation.pts, &stream)?;
            if previous_end.is_some_and(|end| end != start) {
                return Err(
                    AudioIndexError::Metadata("discontinuous physical decode positions").into(),
                );
            }
            let count = i64::from(observation.sample_count);
            let duration = observation
                .reported_duration
                .filter(|duration| *duration > 0)
                .ok_or(AudioIndexError::Metadata("missing measured frame duration"))?;
            let duration_samples = sample_position(duration, &stream)?;
            if duration_samples <= 0 || duration_samples > count {
                return Err(
                    AudioIndexError::Metadata("frame duration exceeds physical samples").into(),
                );
            }
            let skip = observation.skip_samples;
            let mut leading = i64::from(skip.map_or(0, |skip| skip.leading));
            let trailing = i64::from(skip.map_or(0, |skip| skip.trailing));
            if leading + trailing > count {
                return Err(AudioIndexError::Metadata(
                    "cross-frame skip requires further qualification",
                )
                .into());
            }
            let end_offset = duration_samples.min(count - trailing);
            // A completely skipped priming frame has no valid coverage. Other
            // contradictory duration/skip claims must not silently trim content.
            if observation.discard {
                leading = count;
            }
            let end_offset = if leading == count {
                leading
            } else {
                end_offset
            };
            if leading > end_offset {
                return Err(
                    AudioIndexError::Metadata("conflicting frame duration and skip").into(),
                );
            }
            let checked = |offset: i64| {
                start
                    .checked_add(offset)
                    .ok_or(AudioIndexError::Metadata("sample coordinate overflow"))
            };
            frames.push(IndexedAudioFrame {
                source_start: start,
                valid_start: checked(leading)?,
                valid_end: checked(end_offset)?,
                cache_start: decoded_samples,
            });
            previous_end = Some(checked(count)?);
            decoded_samples = decoded_samples
                .checked_add(u64::from(observation.sample_count))
                .ok_or(AudioIndexError::Limit)?;
            valid_samples = valid_samples
                .checked_add((end_offset - leading) as u64)
                .ok_or(AudioIndexError::Limit)?;
        }
        if valid_samples == 0 {
            return Err(AudioIndexError::Metadata("no measured valid samples").into());
        }
        check()?;
        Ok(Self {
            schema_version: AUDIO_INDEX_VERSION,
            decoder_contract: AUDIO_DECODER_CONTRACT.into(),
            content,
            stream,
            observations,
            frames,
            decoded_samples,
            valid_samples,
        })
    }

    pub fn content(&self) -> SourceContentIdentity {
        self.content
    }
    pub fn stream(&self) -> &AudioStreamDescriptor {
        &self.stream
    }
    pub fn observations(&self) -> &[AudioFrameObservation] {
        &self.observations
    }
    pub fn frames(&self) -> &[IndexedAudioFrame] {
        &self.frames
    }
    pub fn decoded_samples(&self) -> u64 {
        self.decoded_samples
    }
    pub fn valid_samples(&self) -> u64 {
        self.valid_samples
    }

    pub fn from_json(bytes: &[u8]) -> Result<Self, AudioIndexError> {
        if bytes.len() > MAX_SOURCE_INDEX_JSON_BYTES {
            return Err(AudioIndexError::Limit);
        }
        Ok(serde_json::from_slice(bytes)?)
    }

    pub fn to_json(&self) -> Result<Vec<u8>, AudioIndexError> {
        let mut writer = crate::source_index::IndexWriter(Vec::new());
        serde_json::to_writer(&mut writer, self).map_err(|error| {
            if error.is_io() {
                AudioIndexError::Limit
            } else {
                AudioIndexError::Json(error)
            }
        })?;
        Ok(writer.0)
    }
}

fn sample_position(ticks: i64, stream: &AudioStreamDescriptor) -> Result<i64, AudioIndexError> {
    let numerator = i128::from(ticks)
        .checked_mul(i128::from(stream.time_base.numerator()))
        .and_then(|value| value.checked_mul(i128::from(stream.sample_rate)))
        .ok_or(AudioIndexError::Metadata("sample coordinate overflow"))?;
    let denominator = i128::from(stream.time_base.denominator());
    if numerator % denominator != 0 {
        return Err(AudioIndexError::Metadata(
            "nonintegral original sample coordinate",
        ));
    }
    i64::try_from(numerator / denominator)
        .map_err(|_| AudioIndexError::Metadata("sample coordinate overflow"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn measured() -> AudioIndexSnapshot {
        AudioIndexSnapshot::new(
            SourceContentIdentity::new([3; 32], 100).unwrap(),
            AudioStreamDescriptor {
                stream_index: 32,
                codec: "aac".into(),
                time_base: SourceTimeBase::new(1, 48000).unwrap(),
                sample_rate: 48000,
                channel_layout: AudioChannelLayout::Native {
                    channels: 2,
                    mask: 3,
                },
                stream_start: Some(123),
                stream_duration: Some(999999),
                initial_padding: 0,
                trailing_padding: 0,
                seek_preroll: 0,
            },
            [-4, 0, 4]
                .into_iter()
                .map(|pts| AudioFrameObservation {
                    pts,
                    discard: false,
                    decode_timestamp: None,
                    reported_duration: Some(if pts == 4 { 2 } else { 4 }),
                    sample_count: 4,
                    sample_format: "fltp".into(),
                    skip_samples: if pts == -4 {
                        Some(AudioSkipSamples {
                            leading: 4,
                            trailing: 0,
                            leading_reason: 0,
                            trailing_reason: 0,
                        })
                    } else {
                        None
                    },
                })
                .collect(),
        )
        .unwrap()
    }

    #[test]
    fn derived_coverage_uses_measured_duration_and_explicit_skip_not_container_claims() {
        let index = measured();
        assert_eq!(index.decoded_samples(), 12);
        assert_eq!(index.valid_samples(), 6);
        assert_eq!(
            index.frames()[0],
            IndexedAudioFrame {
                source_start: -4,
                valid_start: 0,
                valid_end: 0,
                cache_start: 0
            }
        );
        assert_eq!(
            index.frames()[2],
            IndexedAudioFrame {
                source_start: 4,
                valid_start: 4,
                valid_end: 6,
                cache_start: 8
            }
        );
        assert_eq!(
            AudioIndexSnapshot::from_json(&index.to_json().unwrap()).unwrap(),
            index
        );
        let mut value = serde_json::to_value(&index).unwrap();
        value["observations"][0]["skip_samples"] = serde_json::Value::Null;
        value["observations"][0]["discard"] = true.into();
        assert_eq!(
            AudioIndexSnapshot::from_json(&serde_json::to_vec(&value).unwrap())
                .unwrap()
                .valid_samples(),
            6
        );
    }

    #[test]
    fn malformed_cache_cannot_forge_timing_layout_or_derived_offsets() {
        for change in 0..17 {
            let mut value = serde_json::to_value(measured()).unwrap();
            match change {
                0 => value["schema_version"] = 2.into(),
                1 => value["decoder_contract"] = "unqualified".into(),
                2 => value["content"]["byte_length"] = 0.into(),
                3 => value["stream"]["channel_layout"]["mask"] = 1.into(),
                4 => value["stream"]["sample_rate"] = 0.into(),
                5 => value["stream"]["stream_index"] = 33.into(),
                6 => value["observations"][1]["pts"] = 1.into(),
                7 => value["observations"][2]["reported_duration"] = 5.into(),
                8 => value["observations"][2]["reported_duration"] = serde_json::Value::Null,
                9 => value["observations"][0]["skip_samples"]["leading"] = 5.into(),
                10 => value["observations"][0]["sample_count"] = 65537.into(),
                11 => value["frames"] = serde_json::json!([]),
                12 => value["decoded_samples"] = 1.into(),
                13 => {
                    value["stream"]["time_base"] =
                        serde_json::json!({"numerator":1,"denominator":44100})
                }
                14 => value["observations"][0]["sample_format"] = "s16".into(),
                15 => value["observations"] = serde_json::json!([]),
                _ => value["observations"][0]["reported_duration"] = 0.into(),
            }
            assert!(
                AudioIndexSnapshot::from_json(&serde_json::to_vec(&value).unwrap()).is_err(),
                "case {change}"
            );
        }
    }

    #[test]
    fn exact_index_rejects_coordinate_overflow_and_preserves_interior_exclusions() {
        let original = measured();
        let mut observations = original.observations().to_vec();
        observations[0].pts = i64::MAX;
        assert!(
            AudioIndexSnapshot::new(original.content(), original.stream().clone(), observations)
                .is_err()
        );
        let mut observations = original.observations().to_vec();
        observations[1].skip_samples = Some(AudioSkipSamples {
            leading: 1,
            trailing: 1,
            leading_reason: 1,
            trailing_reason: 0,
        });
        let index =
            AudioIndexSnapshot::new(original.content(), original.stream().clone(), observations)
                .unwrap();
        assert_eq!(
            (index.frames()[1].valid_start, index.frames()[1].valid_end),
            (1, 3)
        );
        assert_eq!(index.frames()[2].valid_start, 4);
    }

    #[test]
    fn controlled_index_stops_between_observations_before_publishing() {
        let original = measured();
        let mut checkpoints = 0;
        let result = AudioIndexSnapshot::new_controlled(
            original.content(),
            original.stream().clone(),
            original.observations().to_vec(),
            || {
                checkpoints += 1;
                if checkpoints == 3 {
                    Err(AudioIndexError::Limit)
                } else {
                    Ok(())
                }
            },
        );
        assert!(matches!(result, Err(AudioIndexError::Limit)));
        assert_eq!(checkpoints, 3);
    }
}
