//! Durable, audio-only context for a frozen reference. This records authored
//! intent and immutable asset contracts, not permission to open media bytes.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use crate::document::unique_map;
use crate::{
    AssetId, AssetRecord, AudioSample, DocumentError, DocumentErrorCode, FrozenAudioKind,
    FrozenAudioLayout, HoldAudio, MAX_DOCUMENT_ASSETS, MAX_DOCUMENT_JSON_BYTES, MAX_DOCUMENT_NODES,
    NodeId, NodeKind, ProjectDocument, ProjectId, ReferenceAudibility, RevisionId, SourceAudio,
    SourceAudioMapping,
};

const AUDIO_CONTEXT_SCHEMA: u32 = 3;

/// Full authored audio input. Source mapping and signed mix offset are retained
/// because their effective placement need not fit SourceAudioMapping::Placement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum FrozenAudioInput {
    Source {
        source: SourceAudio,
        mapping: SourceAudioMapping,
        offset: AudioSample,
    },
    Hold {
        source: SourceAudio,
    },
}

/// Version 1 predates exact selection windows. Its closed input grammar is
/// independent of the current Source mapping enum, including nested keys.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum LegacyInput {
    Source {
        source: SourceAudio,
        mapping: crate::legacy_audio_mapping_v19::AudioMapping,
        offset: AudioSample,
    },
    Hold {
        source: SourceAudio,
    },
}

impl LegacyInput {
    fn upgrade(self) -> FrozenAudioInput {
        match self {
            Self::Source {
                source,
                mapping,
                offset,
            } => FrozenAudioInput::Source {
                source,
                mapping: mapping.upgrade(),
                offset,
            },
            Self::Hold { source } => FrozenAudioInput::Hold { source },
        }
    }
}

impl FrozenAudioInput {
    pub fn source(&self) -> &SourceAudio {
        match self {
            Self::Source { source, .. } | Self::Hold { source } => source,
        }
    }
}

/// A standalone immutable snapshot of one revision's audio processing inputs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FrozenAudioContext {
    schema_version: u32,
    project_id: ProjectId,
    revision_id: RevisionId,
    layout: FrozenAudioLayout,
    inputs: BTreeMap<NodeId, FrozenAudioInput>,
    assets: BTreeMap<AssetId, AssetRecord>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ContextWire<'a> {
    schema_version: u32,
    project_id: ProjectId,
    revision_id: RevisionId,
    #[serde(borrow)]
    layout: &'a RawValue,
    #[serde(borrow, deserialize_with = "unique_map")]
    inputs: BTreeMap<NodeId, &'a RawValue>,
    #[serde(deserialize_with = "unique_map")]
    assets: BTreeMap<AssetId, AssetRecord>,
}

impl FrozenAudioContext {
    pub fn capture(document: &ProjectDocument) -> Result<Self, DocumentError> {
        document.validate()?;
        if !document.sounds().is_empty() {
            return Err(invalid(
                "audio contexts cannot yet retain authored sound events",
            ));
        }
        if !document.audio_bindings().is_empty() {
            return Err(invalid(
                "audio contexts cannot retain authored audio timing bindings",
            ));
        }
        let layout = FrozenAudioLayout::capture(document)?;
        // The layout has already charged nodes, edges, compact runs and its
        // serialized size. Count dependencies before cloning source records.
        let mut input_count = 0usize;
        for node in document.nodes().values() {
            match &node.kind {
                NodeKind::Source { source } if source.audio.is_some() => input_count += 1,
                NodeKind::Hold { recipe } if !matches!(recipe.audio, HoldAudio::Silence) => {
                    input_count += 1
                }
                NodeKind::Repeat { gap: Some(gap), .. }
                    if !matches!(gap.audio, HoldAudio::Silence) =>
                {
                    input_count += 1
                }
                _ => {}
            }
        }
        if input_count > MAX_DOCUMENT_NODES {
            return Err(limit("audio context input count exceeds node limit"));
        }
        let mut inputs = BTreeMap::new();
        let mut asset_ids = BTreeSet::new();
        for (id, node) in document.nodes() {
            let input = match &node.kind {
                NodeKind::Source { source } => {
                    source.audio.as_ref().map(|audio| FrozenAudioInput::Source {
                        source: audio.clone(),
                        mapping: source.audio_mapping,
                        offset: source.audio_offset,
                    })
                }
                NodeKind::Hold { recipe } => {
                    hold_source(&recipe.audio).map(|source| FrozenAudioInput::Hold {
                        source: source.clone(),
                    })
                }
                NodeKind::Repeat { gap: Some(gap), .. } => {
                    hold_source(&gap.audio).map(|source| FrozenAudioInput::Hold {
                        source: source.clone(),
                    })
                }
                _ => None,
            };
            if let Some(input) = input {
                asset_ids.insert(input.source().asset.clone());
                inputs.insert(id.clone(), input);
            }
        }
        if asset_ids.len() > MAX_DOCUMENT_ASSETS {
            return Err(limit("audio context asset count exceeds asset limit"));
        }
        let assets = asset_ids
            .into_iter()
            .map(|id| (id.clone(), document.assets()[&id].clone()))
            .collect();
        let context = Self {
            schema_version: AUDIO_CONTEXT_SCHEMA,
            project_id: document.project_id().clone(),
            revision_id: document.revision_id().clone(),
            layout,
            inputs,
            assets,
        };
        context.validate()?;
        context.to_json()?;
        Ok(context)
    }

    pub fn from_json(json: &str) -> Result<Self, DocumentError> {
        if json.len() > MAX_DOCUMENT_JSON_BYTES {
            return Err(limit("audio context JSON exceeds byte limit"));
        }
        let wire: ContextWire<'_> = serde_json::from_str(json).map_err(DocumentError::json)?;
        if !(1..=AUDIO_CONTEXT_SCHEMA).contains(&wire.schema_version) {
            return Err(DocumentError::new(
                DocumentErrorCode::UnsupportedSchema,
                format!("unsupported audio context schema {}", wire.schema_version),
            ));
        }
        // Layout ingress must use its own streaming preflight before typed
        // materialization. A generic nested Deserialize would bypass that gate.
        if wire.schema_version < 3 {
            crate::legacy_audio_binding_v21::validate_v22_layout(wire.layout.get())
                .map_err(DocumentError::json)?;
        }
        let mut inputs = BTreeMap::new();
        for (id, raw) in wire.inputs {
            if raw.get().len() > 64 * 1024 {
                return Err(limit("audio context input exceeds byte limit"));
            }
            let input = if wire.schema_version == 1 {
                serde_json::from_str::<LegacyInput>(raw.get())
                    .map_err(DocumentError::json)?
                    .upgrade()
            } else {
                serde_json::from_str(raw.get()).map_err(DocumentError::json)?
            };
            inputs.insert(id, input);
        }
        let context = Self {
            schema_version: wire.schema_version,
            project_id: wire.project_id,
            revision_id: wire.revision_id,
            layout: FrozenAudioLayout::from_json(wire.layout.get())?,
            inputs,
            assets: wire.assets,
        };
        context.validate()?;
        Ok(context)
    }

    pub fn to_json(&self) -> Result<String, DocumentError> {
        self.validate()?;
        let mut output = BoundedJson::default();
        serde_json::to_writer(&mut output, self).map_err(|error| {
            if output.exceeded {
                limit("audio context JSON exceeds byte limit")
            } else {
                DocumentError::json(error)
            }
        })?;
        String::from_utf8(output.bytes)
            .map_err(|error| DocumentError::new(DocumentErrorCode::InvalidJson, error.to_string()))
    }

    /// Authenticate every retained fact against a fresh capture. A valid
    /// schema-1 input keeps its original serialization version; that version
    /// alone must not make its unchanged historical body unavailable.
    pub fn matches_document(&self, document: &ProjectDocument) -> Result<bool, DocumentError> {
        self.validate()?;
        let mut captured = Self::capture(document)?;
        captured.schema_version = self.schema_version;
        Ok(*self == captured)
    }

    pub fn validate(&self) -> Result<(), DocumentError> {
        if !(1..=AUDIO_CONTEXT_SCHEMA).contains(&self.schema_version) {
            return Err(DocumentError::new(
                DocumentErrorCode::UnsupportedSchema,
                "unsupported audio context schema",
            ));
        }
        self.layout.validate()?;
        if self.inputs.len() > MAX_DOCUMENT_NODES || self.assets.len() > MAX_DOCUMENT_ASSETS {
            return Err(limit("audio context inventory exceeds document limits"));
        }
        let mut expected_inputs = BTreeSet::new();
        for (id, node) in self.layout.nodes() {
            match &node.kind {
                FrozenAudioKind::Source { placement: Some(_) }
                | FrozenAudioKind::Hold {
                    audio: ReferenceAudibility::RoomTone | ReferenceAudibility::Tail { .. },
                }
                | FrozenAudioKind::Repeat {
                    gap_audio: ReferenceAudibility::RoomTone | ReferenceAudibility::Tail { .. },
                    ..
                } => {
                    expected_inputs.insert(id.clone());
                }
                _ => {}
            }
        }
        if self.inputs.keys().collect::<BTreeSet<_>>() != expected_inputs.iter().collect() {
            return Err(invalid(
                "audio context input inventory disagrees with frozen layout",
            ));
        }
        let expected_assets: BTreeSet<_> = self
            .inputs
            .values()
            .map(|input| &input.source().asset)
            .collect();
        if self.assets.keys().collect::<BTreeSet<_>>() != expected_assets {
            return Err(invalid(
                "audio context asset inventory disagrees with inputs",
            ));
        }
        for (id, asset) in &self.assets {
            ProjectDocument::validate_asset(id, asset)?;
        }
        for (id, input) in &self.inputs {
            let source = input.source();
            if !self
                .assets
                .get(&source.asset)
                .and_then(|asset| asset.audio)
                .is_some_and(|span| span.contains_span(source.span))
            {
                return Err(DocumentError::new(
                    DocumentErrorCode::SourceRangeInvalid,
                    format!("audio input {id} exceeds retained asset stream bounds"),
                ));
            }
            match (&self.layout.nodes()[id].kind, input) {
                (
                    FrozenAudioKind::Source {
                        placement: Some(placement),
                    },
                    FrozenAudioInput::Source {
                        mapping, offset, ..
                    },
                ) => {
                    if self.schema_version == 1
                        && crate::legacy_audio_mapping_v19::AudioMapping::project(*mapping)
                            .is_none()
                    {
                        return Err(invalid(
                            "schema-1 audio context contains a selected placement",
                        ));
                    }
                    let selection = mapping.selection_frames_with_offset(
                        self.layout.nodes()[id].duration,
                        *offset,
                        self.layout.rate(),
                    )?;
                    if selection != *placement {
                        return Err(invalid(
                            "source input mapping disagrees with frozen placement",
                        ));
                    }
                }
                (
                    FrozenAudioKind::Hold {
                        audio: ReferenceAudibility::RoomTone | ReferenceAudibility::Tail { .. },
                    }
                    | FrozenAudioKind::Repeat {
                        gap_audio: ReferenceAudibility::RoomTone | ReferenceAudibility::Tail { .. },
                        ..
                    },
                    FrozenAudioInput::Hold { .. },
                ) => {}
                _ => return Err(invalid("audio input kind disagrees with frozen node")),
            }
        }
        Ok(())
    }

    pub fn project_id(&self) -> &ProjectId {
        &self.project_id
    }
    pub fn revision_id(&self) -> &RevisionId {
        &self.revision_id
    }
    pub fn layout(&self) -> &FrozenAudioLayout {
        &self.layout
    }
    pub fn inputs(&self) -> &BTreeMap<NodeId, FrozenAudioInput> {
        &self.inputs
    }
    pub fn assets(&self) -> &BTreeMap<AssetId, AssetRecord> {
        &self.assets
    }
}

fn hold_source(audio: &HoldAudio) -> Option<&SourceAudio> {
    match audio {
        HoldAudio::Silence => None,
        HoldAudio::RoomTone { source } | HoldAudio::Tail { source, .. } => Some(source),
    }
}
fn invalid(message: &str) -> DocumentError {
    DocumentError::new(DocumentErrorCode::InvalidTree, message)
}
fn limit(message: &str) -> DocumentError {
    DocumentError::new(DocumentErrorCode::LimitExceeded, message)
}

#[derive(Default)]
struct BoundedJson {
    bytes: Vec<u8>,
    exceeded: bool,
}
impl Write for BoundedJson {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > MAX_DOCUMENT_JSON_BYTES.saturating_sub(self.bytes.len()) {
            self.exceeded = true;
            return Err(std::io::Error::other(
                "audio context JSON exceeds byte limit",
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
