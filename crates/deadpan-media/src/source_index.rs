//! Identity-bound, validated serialization of measured source presentation indexes.
//! This is derived media metadata, never an alternative authored project document.

use deadpan_core::{
    AssetId, DocumentError, IndexedSourceFrame, SourceFrameIndex, SourceTimeBase,
    TerminalProvenance,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const SOURCE_INDEX_VERSION: u32 = 1;
pub const MAX_SOURCE_INDEX_JSON_BYTES: usize = 128 * 1024 * 1024;

/// Identity of the complete original input, independent of its pathname.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "SourceContentIdentityWire")]
pub struct SourceContentIdentity {
    sha256: [u8; 32],
    byte_length: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceContentIdentityWire {
    sha256: [u8; 32],
    byte_length: u64,
}

impl TryFrom<SourceContentIdentityWire> for SourceContentIdentity {
    type Error = SourceIndexError;

    fn try_from(value: SourceContentIdentityWire) -> Result<Self, Self::Error> {
        Self::new(value.sha256, value.byte_length)
    }
}

impl SourceContentIdentity {
    pub fn new(sha256: [u8; 32], byte_length: u64) -> Result<Self, SourceIndexError> {
        if byte_length == 0 || byte_length > i64::MAX as u64 {
            return Err(SourceIndexError::Identity);
        }
        Ok(Self {
            sha256,
            byte_length,
        })
    }

    pub const fn sha256(self) -> [u8; 32] {
        self.sha256
    }

    pub const fn byte_length(self) -> u64 {
        self.byte_length
    }
}

#[derive(Debug, Error)]
pub enum SourceIndexError {
    #[error("source content identity has an invalid byte length")]
    Identity,
    #[error("source index schema or selected stream is unsupported")]
    Schema,
    #[error("source index JSON exceeds its byte budget")]
    ByteLimit,
    #[error(transparent)]
    Document(#[from] DocumentError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// Only checked constructors/deserialization can produce this envelope.
/// Loading metadata does not establish that its media is present: compare the
/// full input identity and decoder contract before reusing a cached index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "SourceIndexSnapshotWire")]
pub struct SourceIndexSnapshot {
    schema_version: u32,
    content: SourceContentIdentity,
    stream_index: u32,
    index: SourceFrameIndex,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceIndexSnapshotWire {
    schema_version: u32,
    content: SourceContentIdentity,
    stream_index: u32,
    index: FrameIndexWire,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FrameIndexWire {
    asset: AssetId,
    time_base: SourceTimeBase,
    frames: Vec<IndexedSourceFrame>,
    terminal_end: i64,
    terminal_provenance: TerminalProvenance,
}

impl TryFrom<SourceIndexSnapshotWire> for SourceIndexSnapshot {
    type Error = SourceIndexError;

    fn try_from(value: SourceIndexSnapshotWire) -> Result<Self, Self::Error> {
        if value.schema_version != SOURCE_INDEX_VERSION {
            return Err(SourceIndexError::Schema);
        }
        let index = value.index;
        Self::new(
            value.content,
            value.stream_index,
            SourceFrameIndex::new(
                index.asset,
                index.time_base,
                index.frames,
                index.terminal_end,
                index.terminal_provenance,
            )?,
        )
    }
}

impl SourceIndexSnapshot {
    pub fn new(
        content: SourceContentIdentity,
        stream_index: u32,
        index: SourceFrameIndex,
    ) -> Result<Self, SourceIndexError> {
        // Native admits one video plus up to 32 audio streams, in any order.
        if stream_index >= 33 {
            return Err(SourceIndexError::Schema);
        }
        Ok(Self {
            schema_version: SOURCE_INDEX_VERSION,
            content,
            stream_index,
            index,
        })
    }

    pub const fn content(&self) -> SourceContentIdentity {
        self.content
    }

    pub const fn stream_index(&self) -> u32 {
        self.stream_index
    }

    pub fn index(&self) -> &SourceFrameIndex {
        &self.index
    }

    /// Apply the byte bound before parsing untrusted cache data. The complete
    /// index is then reconstructed through the core presentation invariants.
    pub fn from_json(bytes: &[u8]) -> Result<Self, SourceIndexError> {
        if bytes.len() > MAX_SOURCE_INDEX_JSON_BYTES {
            return Err(SourceIndexError::ByteLimit);
        }
        Ok(serde_json::from_slice(bytes)?)
    }

    pub fn to_json(&self) -> Result<Vec<u8>, SourceIndexError> {
        let mut writer = IndexWriter(Vec::new());
        serde_json::to_writer(&mut writer, self).map_err(|error| {
            if error.is_io() {
                SourceIndexError::ByteLimit
            } else {
                SourceIndexError::Json(error)
            }
        })?;
        Ok(writer.0)
    }
}

pub(crate) struct IndexWriter(pub(crate) Vec<u8>);

impl std::io::Write for IndexWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > MAX_SOURCE_INDEX_JSON_BYTES.saturating_sub(self.0.len()) {
            return Err(std::io::Error::other("source index exceeds byte budget"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use deadpan_core::SourceFrameId;

    use super::*;

    fn snapshot() -> SourceIndexSnapshot {
        SourceIndexSnapshot::new(
            SourceContentIdentity::new([7; 32], 1024).unwrap(),
            0,
            SourceFrameIndex::new(
                AssetId::new("original").unwrap(),
                SourceTimeBase::new(1, 30_000).unwrap(),
                [-2002, -1001, 1001]
                    .into_iter()
                    .enumerate()
                    .map(|(ordinal, pts)| IndexedSourceFrame {
                        identity: SourceFrameId(ordinal as u64),
                        pts,
                        reported_duration: Some(1001),
                        keyframe: ordinal == 0,
                        seek_from: Some(SourceFrameId(0)),
                        decode_timestamp: None,
                    })
                    .collect(),
                2002,
                TerminalProvenance::DecodedFrameDuration,
            )
            .unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn roundtrip_preserves_original_clock_identity_and_terminal_evidence() {
        let original = snapshot();
        let decoded = SourceIndexSnapshot::from_json(&original.to_json().unwrap()).unwrap();
        assert_eq!(original, decoded);
        assert_eq!(
            decoded.index().interval(SourceFrameId(1)).unwrap(),
            (-1001, 1001)
        );
        assert_eq!(decoded.content().sha256(), [7; 32]);
    }

    #[test]
    fn final_native_stream_slot_is_representable() {
        let original = snapshot();
        let last =
            SourceIndexSnapshot::new(original.content(), 32, original.index().clone()).unwrap();
        assert_eq!(
            SourceIndexSnapshot::from_json(&last.to_json().unwrap())
                .unwrap()
                .stream_index(),
            32
        );
        assert!(
            SourceIndexSnapshot::new(original.content(), 33, original.index().clone()).is_err()
        );
    }

    #[test]
    fn cache_json_cannot_bypass_core_index_invariants_or_versioning() {
        for change in 0..7 {
            let mut value = serde_json::to_value(snapshot()).unwrap();
            match change {
                0 => value["index"]["frames"][1]["pts"] = (-2002).into(),
                1 => value["index"]["frames"][1]["identity"] = 0.into(),
                2 => value["index"]["frames"][1]["seek_from"] = 2.into(),
                3 => value["index"]["terminal_end"] = 1001.into(),
                4 => value["schema_version"] = 2.into(),
                5 => value["content"]["byte_length"] = 0.into(),
                _ => value["stream_index"] = 33.into(),
            }
            assert!(SourceIndexSnapshot::from_json(&serde_json::to_vec(&value).unwrap()).is_err());
        }
        let mut bytes = String::from_utf8(snapshot().to_json().unwrap()).unwrap();
        bytes = bytes.replacen(
            "\"schema_version\":1",
            "\"schema_version\":1,\"schema_version\":1",
            1,
        );
        assert!(SourceIndexSnapshot::from_json(bytes.as_bytes()).is_err());
    }
}
