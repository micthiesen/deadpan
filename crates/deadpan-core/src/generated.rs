//! Pure identities and exact sampling metadata for accepted generated holds.
//! Filesystem ownership, media validation, and candidate admission belong to hosts.

use std::{error::Error, fmt};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::{AssetId, ExactRatio, FrameDuration, FrameRate};

const ALGORITHM: &str = "blake3";
const DIGEST_HEX_BYTES: usize = 64;
const SAMPLING_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeneratedErrorCode {
    InvalidContentId,
    InvalidObjectLength,
    InvalidSamplingMap,
    SamplingIndexOutOfRange,
    SamplingOverflow,
}

impl GeneratedErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidContentId => "GeneratedContentIdInvalid",
            Self::InvalidObjectLength => "GeneratedObjectInvalid",
            Self::InvalidSamplingMap => "GeneratedSamplingInvalid",
            Self::SamplingIndexOutOfRange => "GeneratedSamplingIndexOutOfRange",
            Self::SamplingOverflow => "GeneratedSamplingOverflow",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedError {
    pub code: GeneratedErrorCode,
    pub message: String,
}

impl GeneratedError {
    fn new(code: GeneratedErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for GeneratedError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for GeneratedError {}

/// A validated BLAKE3 content identity.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GeneratedContentId {
    digest: String,
}

impl GeneratedContentId {
    pub fn new(digest: String) -> Result<Self, GeneratedError> {
        if digest.len() != DIGEST_HEX_BYTES
            || !digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(GeneratedError::new(
                GeneratedErrorCode::InvalidContentId,
                "generated content ID must be exactly 64 lowercase hexadecimal characters",
            ));
        }
        Ok(Self { digest })
    }

    pub const fn algorithm(&self) -> &'static str {
        ALGORITHM
    }

    pub fn digest(&self) -> &str {
        &self.digest
    }
}

impl fmt::Display for GeneratedContentId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{ALGORITHM}:{}", self.digest)
    }
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct ContentIdSerialize<'a> {
    algorithm: &'static str,
    digest: &'a str,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ContentIdDeserialize {
    algorithm: String,
    digest: String,
}

impl Serialize for GeneratedContentId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        ContentIdSerialize {
            algorithm: ALGORITHM,
            digest: &self.digest,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for GeneratedContentId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = ContentIdDeserialize::deserialize(deserializer)?;
        if wire.algorithm != ALGORITHM {
            return Err(de::Error::custom(
                "generated content algorithm must be blake3",
            ));
        }
        Self::new(wire.digest).map_err(de::Error::custom)
    }
}

/// Immutable identity and exact byte length of a generated object.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GeneratedObjectRef {
    content: GeneratedContentId,
    byte_length: u64,
}

impl GeneratedObjectRef {
    pub fn new(content: GeneratedContentId, byte_length: u64) -> Result<Self, GeneratedError> {
        if byte_length == 0 {
            return Err(GeneratedError::new(
                GeneratedErrorCode::InvalidObjectLength,
                "generated object byte length must be positive",
            ));
        }
        Ok(Self {
            content,
            byte_length,
        })
    }

    pub fn content(&self) -> &GeneratedContentId {
        &self.content
    }

    pub const fn byte_length(&self) -> u64 {
        self.byte_length
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ObjectRefWire {
    content: GeneratedContentId,
    byte_length: u64,
}

impl Serialize for GeneratedObjectRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        ObjectRefWire {
            content: self.content.clone(),
            byte_length: self.byte_length,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for GeneratedObjectRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = ObjectRefWire::deserialize(deserializer)?;
        Self::new(wire.content, wire.byte_length).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BridgeInterpolation {
    EncodedSrgbRgb8LinearHalfUp,
}

/// Exact mapping from authored interior project frames to a native generated sequence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(into = "BridgeSamplingMapWire")]
pub struct BridgeSamplingMap {
    project_rate: FrameRate,
    native_rate: FrameRate,
    native_frame_count: FrameDuration,
    output_frame_count: FrameDuration,
    interpolation: BridgeInterpolation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BridgeSamplingMapWire {
    schema_version: u32,
    project_rate: FrameRate,
    native_rate: FrameRate,
    native_frame_count: FrameDuration,
    output_frame_count: FrameDuration,
    interpolation: BridgeInterpolation,
}

impl BridgeSamplingMap {
    pub fn new(
        project_rate: FrameRate,
        native_rate: FrameRate,
        native_frame_count: FrameDuration,
        output_frame_count: FrameDuration,
        interpolation: BridgeInterpolation,
    ) -> Result<Self, GeneratedError> {
        if native_frame_count.frames() < 2 || output_frame_count == FrameDuration::ZERO {
            return Err(GeneratedError::new(
                GeneratedErrorCode::InvalidSamplingMap,
                "bridge sampling requires at least two native frames and one output frame",
            ));
        }
        Ok(Self {
            project_rate,
            native_rate,
            native_frame_count,
            output_frame_count,
            interpolation,
        })
    }

    pub const fn project_rate(&self) -> FrameRate {
        self.project_rate
    }

    pub const fn native_rate(&self) -> FrameRate {
        self.native_rate
    }

    pub const fn native_frame_count(&self) -> FrameDuration {
        self.native_frame_count
    }

    pub const fn output_frame_count(&self) -> FrameDuration {
        self.output_frame_count
    }

    pub const fn interpolation(&self) -> BridgeInterpolation {
        self.interpolation
    }

    /// Native continuous-frame position `(j+1)*(M-1)/(N+1)` for output `j`.
    pub fn native_position(&self, output_index: i64) -> Result<ExactRatio, GeneratedError> {
        if output_index < 0 || output_index >= self.output_frame_count.frames() {
            return Err(GeneratedError::new(
                GeneratedErrorCode::SamplingIndexOutOfRange,
                "bridge output index is outside the authored frame interval",
            ));
        }
        let numerator = i128::from(output_index)
            .checked_add(1)
            .and_then(|value| value.checked_mul(i128::from(self.native_frame_count.frames() - 1)))
            .ok_or_else(|| {
                GeneratedError::new(
                    GeneratedErrorCode::SamplingOverflow,
                    "bridge sampling coordinate overflowed",
                )
            })?;
        ExactRatio::new(numerator, i128::from(self.output_frame_count.frames()) + 1).map_err(|_| {
            GeneratedError::new(
                GeneratedErrorCode::SamplingOverflow,
                "bridge sampling coordinate overflowed",
            )
        })
    }
}

impl From<BridgeSamplingMap> for BridgeSamplingMapWire {
    fn from(value: BridgeSamplingMap) -> Self {
        Self {
            schema_version: SAMPLING_SCHEMA_VERSION,
            project_rate: value.project_rate,
            native_rate: value.native_rate,
            native_frame_count: value.native_frame_count,
            output_frame_count: value.output_frame_count,
            interpolation: value.interpolation,
        }
    }
}

impl<'de> Deserialize<'de> for BridgeSamplingMap {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = BridgeSamplingMapWire::deserialize(deserializer)?;
        if wire.schema_version != SAMPLING_SCHEMA_VERSION {
            return Err(de::Error::custom("unsupported bridge sampling schema"));
        }
        Self::new(
            wire.project_rate,
            wire.native_rate,
            wire.native_frame_count,
            wire.output_frame_count,
            wire.interpolation,
        )
        .map_err(de::Error::custom)
    }
}

/// Immutable generated media and provenance retained by an accepted Hold.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneratedArtifact {
    pub sampled_asset: AssetId,
    pub sampled_object: GeneratedObjectRef,
    pub native_asset: AssetId,
    pub native_object: GeneratedObjectRef,
    pub provenance: GeneratedObjectRef,
    pub sampling: BridgeSamplingMap,
}
