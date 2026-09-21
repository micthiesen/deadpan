//! Captured decode evidence, separated from its durable metadata representation.
//!
//! Only live media sessions can produce `DecodedSourceQualification`. A loaded
//! snapshot validates retained evidence, but cannot mint that live token. Neither
//! type establishes original-file ownership, asset registration, or readiness.

use std::io::Write;

use deadpan_core::{
    AssetId, DocumentError, ExactRatio, FrameRate, SourceFrameIndex, SourceTimeBase, TimeError,
};
use deadpan_source::{
    ColorMatrix, ColorMetadata, ColorPrimaries, ColorRange, ColorTransfer, SourceAudioStreamInfo,
    SourceStreamInfo,
};
use serde::{Deserialize, Serialize};

use crate::audio_index::AudioIndexSnapshot;
use crate::audio_session::AudioSession;
use crate::source_import_timing::{
    BasisCandidate, GeometryCandidate, ImportTiming, ImportTimingError, derive_import_timing,
    derive_presentation_basis, derive_presentation_geometry,
};
use crate::source_index::{
    MAX_SOURCE_INDEX_JSON_BYTES, SourceContentIdentity, SourceIndexError, SourceIndexSnapshot,
};
use crate::source_session::SourceSession;

pub const SOURCE_QUALIFICATION_VERSION: u32 = 1;
pub const SOURCE_QUALIFICATION_DECODER_CONTRACT: &str = "ffmpeg-8.0.3/source-decoded-v1";
pub const SOURCE_IMPORT_TIMING_POLICY_VERSION: u32 = 1;
pub const MAX_SOURCE_QUALIFICATION_JSON_BYTES: usize = 192 * 1024 * 1024;
pub const QUALIFIED_SOURCE_ASSET_ID: &str = "qualified-source";

#[derive(Debug, thiserror::Error)]
pub enum SourceQualificationError {
    #[error("unsupported source qualification schema, decoder or timing policy")]
    Version,
    #[error("source qualification metadata exceeds its byte or count limit")]
    Limit,
    #[error("invalid source qualification metadata: {0}")]
    Metadata(&'static str),
    #[error(transparent)]
    Timing(#[from] ImportTimingError),
    #[error(transparent)]
    Index(#[from] SourceIndexError),
    #[error(transparent)]
    Document(#[from] DocumentError),
    #[error(transparent)]
    Time(#[from] TimeError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// Evidence captured from sessions that actually decoded the selected streams.
/// There is deliberately no arbitrary constructor or deserialization path.
///
/// ```compile_fail
/// use deadpan_media::source_qualification::DecodedSourceQualification;
/// let forged = serde_json::from_str::<DecodedSourceQualification>("{}");
/// ```
#[derive(Debug)]
pub struct DecodedSourceQualification {
    snapshot: SourceQualificationSnapshot,
}

impl DecodedSourceQualification {
    pub fn from_sessions(
        video: Option<&SourceSession>,
        audio: Option<&AudioSession>,
    ) -> Result<Self, SourceQualificationError> {
        // Reject oversized evidence before cloning either potentially large
        // index. Validation below also counts the final canonical envelope.
        let mut indexed_bytes = 0_usize;
        if let Some(session) = video {
            let mut writer = BoundedWriter::new(MAX_SOURCE_INDEX_JSON_BYTES, false);
            write_json(session.index(), &mut writer)?;
            indexed_bytes = writer.length;
        }
        if let Some(session) = audio {
            let mut writer = BoundedWriter::new(MAX_SOURCE_INDEX_JSON_BYTES, false);
            write_json(session.index(), &mut writer)?;
            indexed_bytes = indexed_bytes
                .checked_add(writer.length)
                .ok_or(SourceQualificationError::Limit)?;
        }
        if indexed_bytes > MAX_SOURCE_QUALIFICATION_JSON_BYTES {
            return Err(SourceQualificationError::Limit);
        }
        let origin_seconds = derive_import_timing(
            video.map(SourceSession::index),
            audio.map(AudioSession::index),
            FrameRate::new(1, 1)?,
        )?
        .origin_seconds;
        let content = video
            .map(|session| session.index().content())
            .or_else(|| audio.map(|session| session.index().content()))
            .ok_or(ImportTimingError::NoStreams)?;
        let video = video
            .map(|session| {
                let index = session.index().index();
                Ok::<_, SourceQualificationError>(QualifiedVideoSnapshot {
                    index: SourceIndexSnapshot::new(
                        content,
                        session.index().stream_index(),
                        SourceFrameIndex::new(
                            AssetId::new(QUALIFIED_SOURCE_ASSET_ID)?,
                            index.time_base(),
                            index.frames().to_vec(),
                            index.terminal_end(),
                            index.terminal_provenance(),
                        )?,
                    )?,
                    interpretation: session.info().clone(),
                })
            })
            .transpose()?;
        let snapshot = SourceQualificationSnapshot {
            schema_version: SOURCE_QUALIFICATION_VERSION,
            decoder_contract: SOURCE_QUALIFICATION_DECODER_CONTRACT.into(),
            timing_policy_version: SOURCE_IMPORT_TIMING_POLICY_VERSION,
            content,
            origin_seconds,
            video,
            audio: audio.map(|session| session.index().clone()),
        };
        snapshot.validate()?;
        Ok(Self { snapshot })
    }

    pub fn snapshot(&self) -> &SourceQualificationSnapshot {
        &self.snapshot
    }
}

/// Checked retained metadata. Loading this type never proves a fresh decode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceQualificationSnapshot {
    schema_version: u32,
    decoder_contract: String,
    timing_policy_version: u32,
    content: SourceContentIdentity,
    /// The exact original-clock start chosen for normalization. Persist it
    /// explicitly, independently of later project frame-rate choices.
    origin_seconds: ExactRatio,
    video: Option<QualifiedVideoSnapshot>,
    audio: Option<AudioIndexSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QualifiedVideoSnapshot {
    index: SourceIndexSnapshot,
    #[serde(with = "SourceStreamInfoWire")]
    interpretation: SourceStreamInfo,
}

impl QualifiedVideoSnapshot {
    pub fn index(&self) -> &SourceIndexSnapshot {
        &self.index
    }

    /// Original source interpretation, independent of any project output policy.
    pub fn interpretation(&self) -> &SourceStreamInfo {
        &self.interpretation
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SnapshotWire {
    schema_version: u32,
    decoder_contract: String,
    timing_policy_version: u32,
    content: SourceContentIdentity,
    #[serde(deserialize_with = "normalization_origin")]
    origin_seconds: ExactRatio,
    #[serde(deserialize_with = "required_option")]
    video: Option<VideoWire>,
    #[serde(deserialize_with = "required_option")]
    audio: Option<AudioIndexSnapshot>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VideoWire {
    index: SourceIndexSnapshot,
    #[serde(with = "SourceStreamInfoWire")]
    interpretation: SourceStreamInfo,
}

impl SourceQualificationSnapshot {
    pub fn content(&self) -> SourceContentIdentity {
        self.content
    }

    pub fn origin_seconds(&self) -> ExactRatio {
        self.origin_seconds
    }

    pub fn video(&self) -> Option<&QualifiedVideoSnapshot> {
        self.video.as_ref()
    }

    pub fn audio(&self) -> Option<&AudioIndexSnapshot> {
        self.audio.as_ref()
    }

    pub fn derive_timing(
        &self,
        project_rate: FrameRate,
    ) -> Result<ImportTiming, SourceQualificationError> {
        Ok(derive_import_timing(
            self.video.as_ref().map(QualifiedVideoSnapshot::index),
            self.audio.as_ref(),
            project_rate,
        )?)
    }

    pub fn basis_candidate(&self) -> Result<Option<BasisCandidate>, SourceQualificationError> {
        self.video
            .as_ref()
            .map(|video| {
                Ok(derive_presentation_basis(
                    &video.index,
                    &video.interpretation,
                )?)
            })
            .transpose()
    }

    pub fn geometry_candidate(
        &self,
    ) -> Result<Option<GeometryCandidate>, SourceQualificationError> {
        self.video
            .as_ref()
            .map(|video| {
                Ok(derive_presentation_geometry(
                    &video.index,
                    &video.interpretation,
                )?)
            })
            .transpose()
    }

    /// Validate bounded persisted metadata without creating live decode proof.
    pub fn from_json(bytes: &[u8]) -> Result<Self, SourceQualificationError> {
        if bytes.len() > MAX_SOURCE_QUALIFICATION_JSON_BYTES {
            return Err(SourceQualificationError::Limit);
        }
        let wire: SnapshotWire = serde_json::from_slice(bytes)?;
        let snapshot = Self {
            schema_version: wire.schema_version,
            decoder_contract: wire.decoder_contract,
            timing_policy_version: wire.timing_policy_version,
            content: wire.content,
            origin_seconds: wire.origin_seconds,
            video: wire.video.map(|video| QualifiedVideoSnapshot {
                index: video.index,
                interpretation: video.interpretation,
            }),
            audio: wire.audio,
        };
        snapshot.validate()?;
        Ok(snapshot)
    }

    /// Stable struct field order, original observation order and a fixed video
    /// asset alias produce identical bytes independently of caller asset names.
    pub fn to_json(&self) -> Result<Vec<u8>, SourceQualificationError> {
        let mut writer = BoundedWriter::new(MAX_SOURCE_QUALIFICATION_JSON_BYTES, true);
        write_json(self, &mut writer)?;
        Ok(writer.bytes)
    }

    fn validate(&self) -> Result<(), SourceQualificationError> {
        if self.schema_version != SOURCE_QUALIFICATION_VERSION
            || self.decoder_contract != SOURCE_QUALIFICATION_DECODER_CONTRACT
            || self.timing_policy_version != SOURCE_IMPORT_TIMING_POLICY_VERSION
        {
            return Err(SourceQualificationError::Version);
        }
        if let Some(video) = &self.video {
            if video.index.content() != self.content
                || video.index.index().asset().as_str() != QUALIFIED_SOURCE_ASSET_ID
            {
                return Err(SourceQualificationError::Metadata(
                    "video content or canonical alias",
                ));
            }
            validate_video(video)?;
            write_json(
                &video.index,
                &mut BoundedWriter::new(MAX_SOURCE_INDEX_JSON_BYTES, false),
            )?;
        }
        if let Some(audio) = &self.audio {
            if audio.content() != self.content {
                return Err(SourceQualificationError::Metadata("audio content identity"));
            }
            validate_audio(audio)?;
            if let Some(video) = &self.video {
                validate_selected_audio(&video.interpretation, audio)?;
            }
            write_json(
                audio,
                &mut BoundedWriter::new(MAX_SOURCE_INDEX_JSON_BYTES, false),
            )?;
        }
        // Timing validation requires measured video terminal duration and one
        // contiguous available audio span. It never invents priming trims.
        let measured_origin = self.derive_timing(FrameRate::new(1, 1)?)?.origin_seconds;
        if self.origin_seconds != measured_origin {
            return Err(SourceQualificationError::Metadata(
                "normalization origin disagrees with selected measured spans",
            ));
        }
        write_json(
            self,
            &mut BoundedWriter::new(MAX_SOURCE_QUALIFICATION_JSON_BYTES, false),
        )?;
        Ok(())
    }
}

fn validate_video(video: &QualifiedVideoSnapshot) -> Result<(), SourceQualificationError> {
    let info = &video.interpretation;
    if !(1..=8192).contains(&info.width)
        || !(1..=8192).contains(&info.height)
        || info.stream_index >= 33
        || info.stream_index != video.index.stream_index()
        || source_clock(info.time_base_num, info.time_base_den)? != video.index.index().time_base()
        || info.sample_aspect_num == 0
        || info.sample_aspect_den == 0
        || info.sample_aspect_num > i32::MAX as u32
        || info.sample_aspect_den > i32::MAX as u32
        || info.rotation_quarter_turns > 3
        || !matches!(info.codec.as_str(), "h264" | "ffv1")
    {
        return Err(SourceQualificationError::Metadata("video stream contract"));
    }
    validate_observations(info.stream_start, info.stream_duration)?;
    validate_observations(info.container_start, info.container_duration)?;
    if video
        .index
        .index()
        .frames()
        .iter()
        .any(|frame| frame.pts == i64::MIN || frame.decode_timestamp == Some(i64::MIN))
    {
        return Err(SourceQualificationError::Metadata(
            "missing native video timestamp",
        ));
    }
    let rgb = match info.pixel_format.as_str() {
        "gbrp" | "rgb24" | "bgr24" | "rgb0" | "bgr0" | "0rgb" | "0bgr" => true,
        "yuv410p" | "yuv411p" | "yuv420p" | "yuv422p" | "yuv440p" | "yuv444p" | "yuvj411p"
        | "yuvj420p" | "yuvj422p" | "yuvj440p" | "yuvj444p" | "nv12" | "nv21" | "nv16" | "nv24"
        | "nv42" | "yuyv422" | "uyvy422" | "yvyu422" | "uyyvyy411" => false,
        _ => {
            return Err(SourceQualificationError::Metadata(
                "unqualified pixel format",
            ));
        }
    };
    if (rgb && (info.color.matrix != ColorMatrix::Rgb || info.color.range != ColorRange::Full))
        || (!rgb && info.color.matrix == ColorMatrix::Rgb)
    {
        return Err(SourceQualificationError::Metadata(
            "pixel format and color interpretation",
        ));
    }
    if info.audio_streams.len() > 32 {
        return Err(SourceQualificationError::Limit);
    }
    if info.stream_index as usize > info.audio_streams.len()
        || (info.codec == "ffv1" && !info.audio_streams.is_empty())
    {
        return Err(SourceQualificationError::Metadata(
            "complete admitted stream inventory",
        ));
    }
    let mut previous = None;
    for audio in &info.audio_streams {
        if audio.stream_index >= 33
            || audio.stream_index == info.stream_index
            || previous.is_some_and(|index| audio.stream_index <= index)
            || audio.codec != "aac"
            || audio.stream_index as usize > info.audio_streams.len()
            || audio
                .sample_rate
                .is_some_and(|value| value == 0 || value > i32::MAX as u32)
            || audio
                .channel_count
                .is_some_and(|value| value == 0 || value > i32::MAX as u32)
        {
            return Err(SourceQualificationError::Metadata("audio inventory"));
        }
        source_clock(audio.time_base_num, audio.time_base_den)?;
        validate_observations(audio.stream_start, audio.stream_duration)?;
        previous = Some(audio.stream_index);
    }
    Ok(())
}

fn validate_audio(audio: &AudioIndexSnapshot) -> Result<(), SourceQualificationError> {
    let stream = audio.stream();
    source_clock(stream.time_base.numerator(), stream.time_base.denominator())?;
    validate_observations(stream.stream_start, stream.stream_duration)?;
    if [
        stream.initial_padding,
        stream.trailing_padding,
        stream.seek_preroll,
    ]
    .into_iter()
    .any(|value| value > i32::MAX as u32)
        || audio
            .observations()
            .iter()
            .any(|frame| frame.pts == i64::MIN || frame.decode_timestamp == Some(i64::MIN))
    {
        return Err(SourceQualificationError::Metadata(
            "native audio observations",
        ));
    }
    Ok(())
}

fn validate_selected_audio(
    info: &SourceStreamInfo,
    audio: &AudioIndexSnapshot,
) -> Result<(), SourceQualificationError> {
    let stream = audio.stream();
    let observed = info
        .audio_streams
        .iter()
        .find(|observed| observed.stream_index == stream.stream_index)
        .ok_or(SourceQualificationError::Metadata(
            "selected audio missing from video inventory",
        ))?;
    if observed.codec != stream.codec
        || source_clock(observed.time_base_num, observed.time_base_den)? != stream.time_base
        || observed
            .sample_rate
            .is_some_and(|rate| rate != stream.sample_rate)
        || observed
            .channel_count
            .is_some_and(|channels| channels != stream.channel_layout.channels())
        || observed.stream_start != stream.stream_start
        || observed.stream_duration != stream.stream_duration
    {
        return Err(SourceQualificationError::Metadata(
            "selected audio disagrees with inventory",
        ));
    }
    Ok(())
}

fn source_clock(
    numerator: u32,
    denominator: u32,
) -> Result<SourceTimeBase, SourceQualificationError> {
    if numerator > i32::MAX as u32 || denominator > i32::MAX as u32 {
        return Err(SourceQualificationError::Metadata("native clock bounds"));
    }
    Ok(SourceTimeBase::new(numerator, denominator)?)
}

fn validate_observations(
    start: Option<i64>,
    duration: Option<i64>,
) -> Result<(), SourceQualificationError> {
    if start == Some(i64::MIN) || duration.is_some_and(|value| value <= 0) {
        return Err(SourceQualificationError::Metadata(
            "native timestamp observation",
        ));
    }
    Ok(())
}

fn required_option<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    Option::deserialize(deserializer)
}

fn normalization_origin<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<ExactRatio, D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct OriginWire {
        numerator: String,
        denominator: String,
    }
    let wire = OriginWire::deserialize(deserializer)?;
    ExactRatio::new(
        wire.numerator.parse().map_err(serde::de::Error::custom)?,
        wire.denominator.parse().map_err(serde::de::Error::custom)?,
    )
    .map_err(serde::de::Error::custom)
}

struct BoundedWriter {
    bytes: Vec<u8>,
    length: usize,
    limit: usize,
    retain: bool,
}

impl BoundedWriter {
    fn new(limit: usize, retain: bool) -> Self {
        Self {
            bytes: Vec::new(),
            length: 0,
            limit,
            retain,
        }
    }
}

impl Write for BoundedWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.length) {
            return Err(std::io::Error::other("qualification metadata byte limit"));
        }
        if self.retain {
            self.bytes
                .try_reserve(bytes.len())
                .map_err(std::io::Error::other)?;
            self.bytes.extend_from_slice(bytes);
        }
        self.length += bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn write_json(
    value: &impl Serialize,
    writer: &mut BoundedWriter,
) -> Result<(), SourceQualificationError> {
    serde_json::to_writer(writer, value).map_err(|error| {
        if error.is_io() {
            SourceQualificationError::Limit
        } else {
            SourceQualificationError::Json(error)
        }
    })
}

// Remote serde definitions preserve the complete native interpretation without
// giving the decoder crate a serialization or authored-document dependency.
#[derive(Serialize, Deserialize)]
#[serde(remote = "SourceStreamInfo", deny_unknown_fields)]
struct SourceStreamInfoWire {
    width: u32,
    height: u32,
    stream_index: u32,
    time_base_num: u32,
    time_base_den: u32,
    sample_aspect_num: u32,
    sample_aspect_den: u32,
    rotation_quarter_turns: u8,
    #[serde(with = "ColorMetadataWire")]
    color: ColorMetadata,
    codec: String,
    pixel_format: String,
    #[serde(deserialize_with = "required_option")]
    stream_start: Option<i64>,
    #[serde(deserialize_with = "required_option")]
    stream_duration: Option<i64>,
    #[serde(deserialize_with = "required_option")]
    container_start: Option<i64>,
    #[serde(deserialize_with = "required_option")]
    container_duration: Option<i64>,
    #[serde(with = "audio_inventory")]
    audio_streams: Vec<SourceAudioStreamInfo>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "SourceAudioStreamInfo", deny_unknown_fields)]
struct SourceAudioStreamInfoWire {
    stream_index: u32,
    codec: String,
    time_base_num: u32,
    time_base_den: u32,
    #[serde(deserialize_with = "required_option")]
    stream_start: Option<i64>,
    #[serde(deserialize_with = "required_option")]
    stream_duration: Option<i64>,
    #[serde(deserialize_with = "required_option")]
    sample_rate: Option<u32>,
    #[serde(deserialize_with = "required_option")]
    channel_count: Option<u32>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "ColorMetadata", deny_unknown_fields)]
struct ColorMetadataWire {
    #[serde(with = "ColorRangeWire")]
    range: ColorRange,
    #[serde(with = "ColorMatrixWire")]
    matrix: ColorMatrix,
    #[serde(with = "ColorTransferWire")]
    transfer: ColorTransfer,
    #[serde(with = "ColorPrimariesWire")]
    primaries: ColorPrimaries,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "ColorRange", rename_all = "snake_case")]
enum ColorRangeWire {
    Limited,
    Full,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "ColorMatrix", rename_all = "snake_case")]
enum ColorMatrixWire {
    Rgb,
    Bt709,
    Bt601,
    Bt2020NonConstant,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "ColorTransfer", rename_all = "snake_case")]
enum ColorTransferWire {
    Bt709,
    Srgb,
    Linear,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "ColorPrimaries", rename_all = "snake_case")]
enum ColorPrimariesWire {
    Bt709,
    Bt2020,
    DisplayP3,
}

mod audio_inventory {
    use super::*;
    use serde::de::{Error, SeqAccess, Visitor};
    use serde::ser::SerializeSeq;

    #[derive(Serialize)]
    struct Borrowed<'a>(#[serde(with = "SourceAudioStreamInfoWire")] &'a SourceAudioStreamInfo);
    #[derive(Deserialize)]
    struct Owned(#[serde(with = "SourceAudioStreamInfoWire")] SourceAudioStreamInfo);

    pub(super) fn serialize<S: serde::Serializer>(
        value: &[SourceAudioStreamInfo],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_seq(Some(value.len()))?;
        for info in value {
            seq.serialize_element(&Borrowed(info))?;
        }
        seq.end()
    }

    pub(super) fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<SourceAudioStreamInfo>, D::Error> {
        struct BoundedInventory;
        impl<'de> Visitor<'de> for BoundedInventory {
            type Value = Vec<SourceAudioStreamInfo>;
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("at most 32 audio inventory entries")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                if seq.size_hint().is_some_and(|count| count > 32) {
                    return Err(A::Error::custom("audio inventory count limit"));
                }
                let mut entries = Vec::new();
                while let Some(entry) = seq.next_element::<Owned>()? {
                    if entries.len() == 32 {
                        return Err(A::Error::custom("audio inventory count limit"));
                    }
                    entries.push(entry.0);
                }
                Ok(entries)
            }
        }
        deserializer.deserialize_seq(BoundedInventory)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_writer_enforces_the_same_bound_with_and_without_retaining_bytes() {
        for retain in [false, true] {
            let mut writer = BoundedWriter::new(4, retain);
            writer.write_all(b"1234").unwrap();
            assert!(writer.write_all(b"5").is_err());
            assert_eq!(writer.length, 4);
            assert_eq!(writer.bytes.len(), if retain { 4 } else { 0 });
        }
    }
}
