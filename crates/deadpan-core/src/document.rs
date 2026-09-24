use std::collections::{BTreeMap, BTreeSet};
use std::{error::Error, fmt};

use serde::{Deserialize, Deserializer, Serialize, de};

use crate::{
    AudioSample, BasisState, FrameDuration, FrameRange, FrameRate, IterationOrder, Mark,
    PlayOverrides, RepeatLayout, SourceAudioMapping, SourceTimestamp, SourceVideoMapping,
    TimeError,
};

pub const DOCUMENT_SCHEMA_VERSION: u32 = 15;
/// Bounds apply before traversal. Structure is walked iteratively, never recursively.
pub const MAX_DOCUMENT_NODES: usize = 100_000;
pub const MAX_DOCUMENT_ASSETS: usize = 100_000;
pub const MAX_DOCUMENT_MARKS: usize = 100_000;
pub const MAX_DOCUMENT_DEPTH: usize = 256;
pub const MAX_DOCUMENT_JSON_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_IDENTITY_BYTES: usize = 128;

macro_rules! identifier {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);
        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, DocumentError> {
                let value = value.into();
                if value.is_empty()
                    || value.len() > MAX_IDENTITY_BYTES
                    || !value
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
                {
                    return Err(DocumentError::new(
                        DocumentErrorCode::InvalidIdentity,
                        concat!(
                            stringify!($name),
                            " must contain 1–128 ASCII letters, digits, hyphens, or underscores"
                        ),
                    ));
                }
                Ok(Self(value))
            }
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
        impl TryFrom<String> for $name {
            type Error = DocumentError;
            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }
        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

identifier!(ProjectId);
identifier!(RevisionId);
identifier!(NodeId);
identifier!(AssetId);
identifier!(MarkId);

/// BLAKE3 identity of a canonical host qualification receipt. The core retains
/// the binding; only the host can establish receipt ownership and validity.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct SourceQualificationId(String);

impl SourceQualificationId {
    pub fn new(value: String) -> Result<Self, DocumentError> {
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(DocumentError::new(
                DocumentErrorCode::InvalidIdentity,
                "source qualification identity must contain exactly 64 lowercase hexadecimal digits",
            ));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for SourceQualificationId {
    type Error = DocumentError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<SourceQualificationId> for String {
    fn from(value: SourceQualificationId) -> Self {
        value.0
    }
}

impl fmt::Display for SourceQualificationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PresentationBasis {
    pub width: u32,
    pub height: u32,
    pub frame_rate: FrameRate,
    pub color_policy: ColorPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorPolicy {
    SdrRec709,
    HdrRec2020Pq,
    HdrRec2020Hlg,
}

/// A half-open interval in one original stream's timestamp clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "SourceSpanWire")]
pub struct SourceSpan {
    start: SourceTimestamp,
    end: SourceTimestamp,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceSpanWire {
    start: SourceTimestamp,
    end: SourceTimestamp,
}

impl TryFrom<SourceSpanWire> for SourceSpan {
    type Error = DocumentError;
    fn try_from(value: SourceSpanWire) -> Result<Self, Self::Error> {
        Self::new(value.start, value.end)
    }
}

impl SourceSpan {
    pub fn new(start: SourceTimestamp, end: SourceTimestamp) -> Result<Self, DocumentError> {
        if start.time_base != end.time_base
            || start.ticks >= end.ticks
            || end.ticks.checked_sub(start.ticks).is_none()
        {
            return Err(DocumentError::new(
                DocumentErrorCode::SourceRangeInvalid,
                "source span must be positive, representable, and use one time base",
            ));
        }
        Ok(Self { start, end })
    }
    pub fn start(self) -> SourceTimestamp {
        self.start
    }
    pub fn end(self) -> SourceTimestamp {
        self.end
    }
    pub fn contains_span(self, span: Self) -> bool {
        self.start.time_base == span.start.time_base
            && self.start.ticks <= span.start.ticks
            && span.end.ticks <= self.end.ticks
    }
    pub fn contains(self, timestamp: SourceTimestamp) -> bool {
        self.start.time_base == timestamp.time_base
            && self.start.ticks <= timestamp.ticks
            && timestamp.ticks < self.end.ticks
    }
}

/// Immutable media identity and measured stream bounds. Locations belong to the host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetRecord {
    pub label: String,
    pub content_hash: String,
    pub video: Option<SourceSpan>,
    pub audio: Option<SourceSpan>,
    pub still_image: bool,
    /// Original presentation-frame count, required for accepted generated intervals.
    pub frame_count: Option<FrameDuration>,
    /// Canonical host receipt retained with an imported original asset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_qualification: Option<SourceQualificationId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SourceVideo {
    Stream { asset: AssetId, span: SourceSpan },
    Still { asset: AssetId },
    Blank,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceAudio {
    pub asset: AssetId,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkRelation {
    Linked,
    Independent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceNode {
    pub duration: FrameDuration,
    pub video: SourceVideo,
    /// Exact picture placement and selected-span endpoint behavior.
    pub video_mapping: SourceVideoMapping,
    pub audio: Option<SourceAudio>,
    /// Explicit audio destination placement, independent of picture timing.
    pub audio_mapping: SourceAudioMapping,
    pub link: LinkRelation,
    /// Signed alignment in the project mix clock; never discard source PTS origins.
    pub audio_offset: AudioSample,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum HoldVideo {
    Background,
    Freeze {
        asset: AssetId,
        timestamp: SourceTimestamp,
    },
    /// References owned, explicitly accepted media, never a model or pending job.
    Accepted {
        asset: AssetId,
        frames: FrameRange,
    },
    /// Explicitly accepted generated media with its original deterministic fallback.
    Generated {
        accepted: Box<AcceptedGeneration>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum HoldFallback {
    Background,
    Freeze {
        asset: AssetId,
        timestamp: SourceTimestamp,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptedGeneration {
    pub artifact: crate::GeneratedArtifact,
    pub fallback: HoldFallback,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum HoldAudio {
    Silence,
    RoomTone {
        source: SourceAudio,
    },
    /// A permitted source tail is distinct from room tone or digital silence.
    Tail {
        source: SourceAudio,
        maximum: FrameDuration,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HoldRecipe {
    pub duration: FrameDuration,
    pub video: HoldVideo,
    pub audio: HoldAudio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PitchPolicy {
    Preserve,
    FollowSpeed,
}

/// An ordinary authored crop constrains audio filtering and introduces edit
/// edges. A transparent partition retains its child's complete processing and
/// envelope domains, exposing only the selected unity-rate output interval.
/// This is a splice building block, not a Split command or a resume anchor.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetimePurpose {
    #[default]
    Edit,
    Partition,
}

impl RetimePurpose {
    pub fn is_edit(&self) -> bool {
        *self == Self::Edit
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum NodeKind {
    Source {
        source: SourceNode,
    },
    Sequence {
        children: Vec<NodeId>,
    },
    Hold {
        recipe: HoldRecipe,
    },
    Repeat {
        child: NodeId,
        iterations: IterationOrder,
        gap: Option<HoldRecipe>,
    },
    Retime {
        child: NodeId,
        duration: FrameDuration,
        mapping: FrameRange,
        pitch: PitchPolicy,
        #[serde(default, skip_serializing_if = "RetimePurpose::is_edit")]
        purpose: RetimePurpose,
    },
}

impl NodeKind {
    pub fn children(&self) -> &[NodeId] {
        match self {
            Self::Sequence { children } => children,
            Self::Repeat { child, .. } | Self::Retime { child, .. } => std::slice::from_ref(child),
            Self::Source { .. } | Self::Hold { .. } => &[],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BeatNode {
    pub label: String,
    pub kind: NodeKind,
    #[serde(
        default,
        skip_serializing_if = "crate::AudioEdgePolicies::is_automatic"
    )]
    pub audio_edges: crate::AudioEdgePolicies,
}

impl BeatNode {
    pub fn sequence(label: impl Into<String>, children: Vec<NodeId>) -> Self {
        Self {
            label: label.into(),
            kind: NodeKind::Sequence { children },
            audio_edges: crate::AudioEdgePolicies::default(),
        }
    }
    pub fn hold(label: impl Into<String>, recipe: HoldRecipe) -> Self {
        Self {
            label: label.into(),
            kind: NodeKind::Hold { recipe },
            audio_edges: crate::AudioEdgePolicies::default(),
        }
    }
}

/// Flat storage keeps JSON depth independent of authored nesting. Only validated
/// documents can be constructed; callers receive immutable maps. JSON ingress
/// goes through `from_json` so a generic deserializer cannot bypass its byte cap.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectDocument {
    pub(crate) schema_version: u32,
    pub(crate) project_id: ProjectId,
    pub(crate) revision_id: RevisionId,
    pub(crate) presentation_basis: PresentationBasis,
    pub(crate) basis_state: BasisState,
    pub(crate) root: NodeId,
    pub(crate) nodes: BTreeMap<NodeId, BeatNode>,
    pub(crate) assets: BTreeMap<AssetId, AssetRecord>,
    pub(crate) marks: BTreeMap<MarkId, Mark>,
    pub(crate) overrides: BTreeMap<NodeId, PlayOverrides>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) audio_lineage: BTreeMap<NodeId, crate::AudioLineageId>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DocumentWire {
    schema_version: u32,
    project_id: ProjectId,
    revision_id: RevisionId,
    presentation_basis: PresentationBasis,
    basis_state: BasisState,
    root: NodeId,
    #[serde(deserialize_with = "unique_map")]
    nodes: BTreeMap<NodeId, BeatNode>,
    #[serde(deserialize_with = "unique_map")]
    assets: BTreeMap<AssetId, AssetRecord>,
    #[serde(deserialize_with = "unique_map")]
    marks: BTreeMap<MarkId, Mark>,
    #[serde(deserialize_with = "unique_map")]
    overrides: BTreeMap<NodeId, PlayOverrides>,
    #[serde(default, deserialize_with = "unique_map")]
    audio_lineage: BTreeMap<NodeId, crate::AudioLineageId>,
}

impl TryFrom<DocumentWire> for ProjectDocument {
    type Error = DocumentError;
    fn try_from(value: DocumentWire) -> Result<Self, Self::Error> {
        let document = Self {
            schema_version: value.schema_version,
            project_id: value.project_id,
            revision_id: value.revision_id,
            presentation_basis: value.presentation_basis,
            basis_state: value.basis_state,
            root: value.root,
            nodes: value.nodes,
            assets: value.assets,
            marks: value.marks,
            overrides: value.overrides,
            audio_lineage: value.audio_lineage,
        };
        document.validate()?;
        Ok(document)
    }
}

impl ProjectDocument {
    pub fn new(
        project_id: ProjectId,
        revision_id: RevisionId,
        presentation_basis: PresentationBasis,
        root: NodeId,
    ) -> Result<Self, DocumentError> {
        let document = Self {
            schema_version: DOCUMENT_SCHEMA_VERSION,
            project_id,
            revision_id,
            presentation_basis,
            basis_state: BasisState::explicit(),
            nodes: BTreeMap::from([(root.clone(), BeatNode::sequence("Sequence", vec![]))]),
            root,
            assets: BTreeMap::new(),
            marks: BTreeMap::new(),
            overrides: BTreeMap::new(),
            audio_lineage: BTreeMap::new(),
        };
        document.validate()?;
        Ok(document)
    }
    /// Start with the audio-only default, still eligible for first-primary adoption.
    pub fn new_automatic(
        project_id: ProjectId,
        revision_id: RevisionId,
        root: NodeId,
    ) -> Result<Self, DocumentError> {
        let mut document = Self::new(project_id, revision_id, crate::basis::default_basis(), root)?;
        document.basis_state = BasisState::provisional();
        Ok(document)
    }
    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }
    pub fn project_id(&self) -> &ProjectId {
        &self.project_id
    }
    pub fn revision_id(&self) -> &RevisionId {
        &self.revision_id
    }
    pub fn presentation_basis(&self) -> &PresentationBasis {
        &self.presentation_basis
    }
    pub fn basis_state(&self) -> &BasisState {
        &self.basis_state
    }
    pub fn root(&self) -> &NodeId {
        &self.root
    }
    pub fn nodes(&self) -> &BTreeMap<NodeId, BeatNode> {
        &self.nodes
    }
    pub fn assets(&self) -> &BTreeMap<AssetId, AssetRecord> {
        &self.assets
    }
    pub fn marks(&self) -> &BTreeMap<MarkId, Mark> {
        &self.marks
    }
    pub fn overrides(&self) -> &BTreeMap<NodeId, PlayOverrides> {
        &self.overrides
    }
    pub fn audio_lineage(&self) -> &BTreeMap<NodeId, crate::AudioLineageId> {
        &self.audio_lineage
    }

    /// All owned structural children, including sparse Repeat override roots.
    /// Use this for tree traversal instead of the primitive-only NodeKind list.
    pub fn children<'a>(&'a self, id: &NodeId) -> impl DoubleEndedIterator<Item = &'a NodeId> {
        self.nodes
            .get(id)
            .into_iter()
            .flat_map(|node| node.kind.children())
            .chain(
                self.overrides
                    .get(id)
                    .into_iter()
                    .flat_map(|entries| entries.iter().map(|(_, root)| root)),
            )
    }

    pub fn to_json(&self) -> Result<String, DocumentError> {
        self.to_json_with_limit(MAX_DOCUMENT_JSON_BYTES)
    }
    fn to_json_with_limit(&self, limit: usize) -> Result<String, DocumentError> {
        use std::io::Write;

        let mut output = BoundedJson {
            bytes: Vec::new(),
            limit,
            exceeded: false,
        };
        let result = serde_json::to_writer_pretty(&mut output, self);
        if output.exceeded {
            return Err(json_limit());
        }
        result.map_err(DocumentError::json)?;
        output.write_all(b"\n").map_err(|_| json_limit())?;
        // serde_json only writes valid UTF-8. Avoid an unchecked conversion so
        // the invariant remains enforced if the serializer ever changes.
        String::from_utf8(output.bytes)
            .map_err(|error| DocumentError::new(DocumentErrorCode::InvalidJson, error.to_string()))
    }
    pub fn from_json(json: &str) -> Result<Self, DocumentError> {
        if json.len() > MAX_DOCUMENT_JSON_BYTES {
            return Err(json_limit());
        }
        let wire: DocumentWire = serde_json::from_str(json).map_err(DocumentError::json)?;
        Self::try_from(wire)
    }

    pub fn validate(&self) -> Result<(), DocumentError> {
        self.durations().map(|_| ())
    }
    pub fn duration(&self) -> Result<FrameDuration, DocumentError> {
        self.node_duration(&self.root)
    }
    pub fn node_duration(&self, id: &NodeId) -> Result<FrameDuration, DocumentError> {
        self.durations()?.get(id).copied().ok_or_else(|| {
            DocumentError::new(
                DocumentErrorCode::MissingNode,
                format!("node {id} does not exist"),
            )
        })
    }

    pub(crate) fn parent_of(&self, id: &NodeId) -> Option<NodeId> {
        self.nodes
            .iter()
            .find(|(parent, _)| self.children(parent).any(|child| child == id))
            .map(|(id, _)| id.clone())
    }

    /// Validate once and return every authored duration for plan compilation.
    pub fn durations(&self) -> Result<BTreeMap<NodeId, FrameDuration>, DocumentError> {
        let durations = self.structural_durations()?;
        self.validate_basis_state(&durations)?;
        crate::audio_lineage::validate(self)?;
        crate::marks::validate_marks(self, &durations)?;
        Ok(durations)
    }

    /// Structural validation precedes mark transforms; no anchor validation
    /// calls back into this traversal or into public `durations()`.
    pub(crate) fn structural_durations(
        &self,
    ) -> Result<BTreeMap<NodeId, FrameDuration>, DocumentError> {
        if self.schema_version != DOCUMENT_SCHEMA_VERSION {
            return Err(DocumentError::new(
                DocumentErrorCode::UnsupportedSchema,
                format!(
                    "unsupported document schema {}; expected {DOCUMENT_SCHEMA_VERSION}",
                    self.schema_version
                ),
            ));
        }
        if self.nodes.len() > MAX_DOCUMENT_NODES || self.assets.len() > MAX_DOCUMENT_ASSETS {
            return Err(DocumentError::new(
                DocumentErrorCode::LimitExceeded,
                "document exceeds 100,000 nodes or assets",
            ));
        }
        if self.overrides.len() > MAX_DOCUMENT_NODES {
            return Err(DocumentError::new(
                DocumentErrorCode::LimitExceeded,
                "too many override owners",
            ));
        }
        for (id, entries) in &self.overrides {
            if entries.is_empty()
                || !matches!(
                    self.nodes.get(id).map(|node| &node.kind),
                    Some(NodeKind::Repeat { .. })
                )
            {
                return Err(DocumentError::new(
                    DocumentErrorCode::InvalidTree,
                    "nonempty overrides must belong to an existing Repeat",
                ));
            }
        }
        let mut edge_count = 0usize;
        for id in self.nodes.keys() {
            edge_count = edge_count
                .checked_add(self.children(id).count())
                .ok_or_else(|| {
                    DocumentError::new(
                        DocumentErrorCode::LimitExceeded,
                        "too many structural references",
                    )
                })?;
            if edge_count > MAX_DOCUMENT_NODES {
                return Err(DocumentError::new(
                    DocumentErrorCode::LimitExceeded,
                    "document exceeds 100,000 structural references",
                ));
            }
        }
        let basis = &self.presentation_basis;
        if basis.width == 0 || basis.height == 0 || basis.width > 65_536 || basis.height > 65_536 {
            return Err(DocumentError::new(
                DocumentErrorCode::InvalidPresentation,
                "presentation dimensions must be in 1..=65536",
            ));
        }
        for (id, asset) in &self.assets {
            self.validate_asset(id, asset)?;
        }
        if !matches!(
            self.nodes.get(&self.root).map(|n| &n.kind),
            Some(NodeKind::Sequence { .. })
        ) {
            return Err(DocumentError::new(
                DocumentErrorCode::InvalidRoot,
                "root must reference a Sequence",
            ));
        }
        let mut seen = BTreeSet::new();
        let mut stack = vec![(self.root.clone(), 0usize, false)];
        let mut durations = BTreeMap::new();
        while let Some((id, depth, visited)) = stack.pop() {
            if depth > MAX_DOCUMENT_DEPTH {
                return Err(DocumentError::new(
                    DocumentErrorCode::LimitExceeded,
                    "document structural depth exceeds 256 edges",
                ));
            }
            let node = self.nodes.get(&id).ok_or_else(|| {
                DocumentError::new(
                    DocumentErrorCode::MissingNode,
                    format!("node {id} does not exist"),
                )
            })?;
            if !visited {
                if !seen.insert(id.clone()) {
                    return Err(DocumentError::new(
                        DocumentErrorCode::InvalidTree,
                        format!("node {id} has multiple parents or forms a cycle"),
                    ));
                }
                validate_label(&node.label)?;
                node.audio_edges.validate(&node.kind)?;
                stack.push((id.clone(), depth, true));
                for child in self.children(&id).rev() {
                    stack.push((child.clone(), depth + 1, false));
                }
                continue;
            }
            let child_duration = |id: &NodeId| -> Result<FrameDuration, DocumentError> {
                durations.get(id).copied().ok_or_else(|| {
                    DocumentError::new(
                        DocumentErrorCode::InvalidTree,
                        format!("child {id} is not evaluated"),
                    )
                })
            };
            let duration = match &node.kind {
                NodeKind::Source { source } => {
                    positive(source.duration, "source")?;
                    match &source.video {
                        SourceVideo::Stream { asset, span } => {
                            self.validate_video_span(asset, *span)?;
                            source.video_mapping.duration_frames(source.duration)?;
                        }
                        SourceVideo::Still { asset } => {
                            if !self.asset(asset)?.still_image {
                                return Err(DocumentError::new(
                                    DocumentErrorCode::SourceRangeInvalid,
                                    format!("asset {asset} is not a still image"),
                                ));
                            }
                        }
                        SourceVideo::Blank => {}
                    }
                    if !matches!(source.video, SourceVideo::Stream { .. })
                        && source.video_mapping != SourceVideoMapping::FitBeat
                    {
                        return Err(DocumentError::new(
                            DocumentErrorCode::SourceRangeInvalid,
                            "an explicit video duration requires selected video",
                        ));
                    }
                    if let Some(audio) = &source.audio {
                        self.validate_audio(audio)?;
                        let frames = source.audio_mapping.duration_frames(source.duration)?;
                        if matches!(source.audio_mapping, SourceAudioMapping::Placement { .. }) {
                            source
                                .audio_mapping
                                .start_frames_with_offset(
                                    source.audio_offset,
                                    self.presentation_basis.frame_rate,
                                )?
                                .checked_add(frames)?;
                        }
                    } else if source.audio_mapping != SourceAudioMapping::FitBeat {
                        return Err(DocumentError::new(
                            DocumentErrorCode::SourceRangeInvalid,
                            "an explicit audio duration requires selected audio",
                        ));
                    }
                    if matches!(source.video, SourceVideo::Blank) && source.audio.is_none() {
                        return Err(DocumentError::new(
                            DocumentErrorCode::SourceRangeInvalid,
                            format!("source {id} has no media; use a Hold for blank time"),
                        ));
                    }
                    if source.link == LinkRelation::Linked
                        && (matches!(source.video, SourceVideo::Blank) || source.audio.is_none())
                    {
                        return Err(DocumentError::new(
                            DocumentErrorCode::SourceRangeInvalid,
                            "linked source requires both picture and audio",
                        ));
                    }
                    source.duration
                }
                NodeKind::Sequence { children } => {
                    let mut sum = FrameDuration::ZERO;
                    for child in children {
                        sum = sum.checked_add(child_duration(child)?)?;
                    }
                    sum
                }
                NodeKind::Hold { recipe } => {
                    self.validate_hold(recipe)?;
                    recipe.duration
                }
                NodeKind::Repeat {
                    child,
                    iterations,
                    gap,
                } => {
                    iterations.validate()?;
                    if let Some(gap) = gap {
                        self.validate_hold(gap)?;
                    }
                    RepeatLayout::compile(
                        iterations,
                        child,
                        self.overrides.get(&id),
                        gap.as_ref().map_or(FrameDuration::ZERO, |gap| gap.duration),
                        &durations,
                    )?
                    .duration()
                }
                NodeKind::Retime {
                    child,
                    duration,
                    mapping,
                    purpose,
                    ..
                } => {
                    positive(*duration, "retime")?;
                    if *purpose == RetimePurpose::Partition
                        && (*duration != mapping.duration() || !node.audio_edges.is_automatic())
                    {
                        return Err(DocumentError::new(
                            DocumentErrorCode::InvalidTree,
                            "a transparent partition requires unity timing and automatic audio edges",
                        ));
                    }
                    let child_frames = child_duration(child)?.frames();
                    if mapping.start().0 < 0
                        || mapping.duration() == FrameDuration::ZERO
                        || mapping.end().0 > child_frames
                    {
                        return Err(DocumentError::new(
                            DocumentErrorCode::SourceRangeInvalid,
                            format!(
                                "retime {id} mapping must be a positive range within child {child}"
                            ),
                        ));
                    }
                    *duration
                }
            };
            durations.insert(id, duration);
        }
        if seen.len() != self.nodes.len() {
            return Err(DocumentError::new(
                DocumentErrorCode::InvalidTree,
                "document contains nodes unreachable from its root",
            ));
        }
        Ok(durations)
    }

    fn validate_asset(&self, id: &AssetId, asset: &AssetRecord) -> Result<(), DocumentError> {
        validate_label(&asset.label)?;
        let sha256 = asset.content_hash.len() == 64
            && asset
                .content_hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        let generated = asset
            .content_hash
            .strip_prefix("blake3:")
            .is_some_and(|digest| crate::GeneratedContentId::new(digest.to_owned()).is_ok());
        if !sha256 && !generated {
            return Err(DocumentError::new(
                DocumentErrorCode::InvalidAsset,
                format!("asset {id} requires a lowercase SHA-256 or typed BLAKE3 content hash"),
            ));
        }
        if asset.video.is_none() && asset.audio.is_none() && !asset.still_image {
            return Err(DocumentError::new(
                DocumentErrorCode::InvalidAsset,
                format!("asset {id} has no supported media stream"),
            ));
        }
        if let Some(count) = asset.frame_count {
            positive(count, "asset frame count")?;
            if asset.video.is_none() {
                return Err(DocumentError::new(
                    DocumentErrorCode::InvalidAsset,
                    format!("asset {id} frame count requires a video stream"),
                ));
            }
        }
        Ok(())
    }
    fn asset(&self, id: &AssetId) -> Result<&AssetRecord, DocumentError> {
        self.assets.get(id).ok_or_else(|| {
            DocumentError::new(
                DocumentErrorCode::MissingAsset,
                format!("asset {id} does not exist"),
            )
        })
    }
    fn validate_video_span(&self, id: &AssetId, span: SourceSpan) -> Result<(), DocumentError> {
        if !self
            .asset(id)?
            .video
            .is_some_and(|bounds| bounds.contains_span(span))
        {
            return Err(DocumentError::new(
                DocumentErrorCode::SourceRangeInvalid,
                format!(
                    "video selection exceeds asset {id} stream bounds or uses a different time base"
                ),
            ));
        }
        Ok(())
    }
    fn validate_audio(&self, audio: &SourceAudio) -> Result<(), DocumentError> {
        if !self
            .asset(&audio.asset)?
            .audio
            .is_some_and(|bounds| bounds.contains_span(audio.span))
        {
            return Err(DocumentError::new(
                DocumentErrorCode::SourceRangeInvalid,
                format!(
                    "audio selection exceeds asset {} stream bounds or uses a different time base",
                    audio.asset
                ),
            ));
        }
        Ok(())
    }
    fn validate_hold(&self, recipe: &HoldRecipe) -> Result<(), DocumentError> {
        positive(recipe.duration, "hold")?;
        match &recipe.video {
            HoldVideo::Background => {}
            HoldVideo::Freeze { asset, timestamp } => {
                if !self
                    .asset(asset)?
                    .video
                    .is_some_and(|bounds| bounds.contains(*timestamp))
                {
                    return Err(DocumentError::new(
                        DocumentErrorCode::SourceRangeInvalid,
                        format!("freeze timestamp is outside asset {asset} video"),
                    ));
                }
            }
            HoldVideo::Accepted { asset, frames } => {
                let record = self.asset(asset)?;
                if frames.start().0 < 0
                    || frames.duration() != recipe.duration
                    || !record
                        .frame_count
                        .is_some_and(|count| frames.end().0 <= count.frames())
                {
                    return Err(DocumentError::new(
                        DocumentErrorCode::SourceRangeInvalid,
                        format!(
                            "accepted artifact {asset} must cover exactly the authored hold duration within its frame bounds"
                        ),
                    ));
                }
            }
            HoldVideo::Generated { accepted } => {
                self.validate_generated(recipe.duration, accepted)?;
            }
        }
        match &recipe.audio {
            HoldAudio::Silence => {}
            HoldAudio::RoomTone { source } => self.validate_audio(source)?,
            HoldAudio::Tail { source, maximum } => {
                self.validate_audio(source)?;
                positive(*maximum, "tail maximum")?;
                if *maximum > recipe.duration {
                    return Err(DocumentError::new(
                        DocumentErrorCode::SourceRangeInvalid,
                        "tail maximum exceeds hold duration",
                    ));
                }
            }
        }
        Ok(())
    }

    fn validate_generated(
        &self,
        duration: FrameDuration,
        accepted: &AcceptedGeneration,
    ) -> Result<(), DocumentError> {
        let artifact = &accepted.artifact;
        if artifact.sampling.project_rate() != self.presentation_basis.frame_rate
            || duration > artifact.sampling.output_frame_count()
        {
            return Err(DocumentError::new(
                DocumentErrorCode::SourceRangeInvalid,
                "generated Hold sampling must use the project rate and cover its authored duration",
            ));
        }
        self.validate_generated_asset(
            &artifact.sampled_asset,
            &artifact.sampled_object,
            artifact.sampling.output_frame_count(),
        )?;
        self.validate_generated_asset(
            &artifact.native_asset,
            &artifact.native_object,
            artifact.sampling.native_frame_count(),
        )?;
        self.validate_hold_fallback(&accepted.fallback)
    }

    fn validate_generated_asset(
        &self,
        id: &AssetId,
        object: &crate::GeneratedObjectRef,
        frames: FrameDuration,
    ) -> Result<(), DocumentError> {
        let record = self.asset(id)?;
        if record.video.is_none()
            || record.audio.is_some()
            || record.still_image
            || record.frame_count != Some(frames)
            || record.content_hash != object.content().to_string()
        {
            return Err(DocumentError::new(
                DocumentErrorCode::InvalidAsset,
                format!(
                    "generated asset {id} must be video-only and exactly match its object and frame count"
                ),
            ));
        }
        Ok(())
    }

    fn validate_hold_fallback(&self, fallback: &HoldFallback) -> Result<(), DocumentError> {
        match fallback {
            HoldFallback::Background => Ok(()),
            HoldFallback::Freeze { asset, timestamp } => {
                if self
                    .asset(asset)?
                    .video
                    .is_some_and(|bounds| bounds.contains(*timestamp))
                {
                    Ok(())
                } else {
                    Err(DocumentError::new(
                        DocumentErrorCode::SourceRangeInvalid,
                        format!("freeze timestamp is outside asset {asset} video"),
                    ))
                }
            }
        }
    }
}

fn json_limit() -> DocumentError {
    DocumentError::new(
        DocumentErrorCode::LimitExceeded,
        "document JSON exceeds its byte limit (64 MiB maximum)",
    )
}

/// Bounds allocation during serialization, including escaping and formatting.
/// This is an in-memory sink; the pure domain layer performs no external I/O.
struct BoundedJson {
    bytes: Vec<u8>,
    limit: usize,
    exceeded: bool,
}

impl std::io::Write for BoundedJson {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            self.exceeded = true;
            return Err(std::io::Error::other("document JSON exceeds byte limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod serialization_tests {
    use super::*;

    #[test]
    fn canonical_json_limit_counts_escaping_utf8_and_terminal_newline() {
        let mut document = ProjectDocument::new(
            ProjectId::new("project").unwrap(),
            RevisionId::new("initial").unwrap(),
            PresentationBasis {
                width: 1920,
                height: 1080,
                frame_rate: FrameRate::new(30, 1).unwrap(),
                color_policy: ColorPolicy::SdrRec709,
            },
            NodeId::new("root").unwrap(),
        )
        .unwrap();
        document
            .nodes
            .get_mut(&NodeId::new("root").unwrap())
            .unwrap()
            .label = "é \"quoted\"\nlabel".into();
        let json = document.to_json().unwrap();
        assert_eq!(document.to_json_with_limit(json.len()).unwrap(), json);
        assert_eq!(ProjectDocument::from_json(&json).unwrap(), document);
        for limit in [0, json.len() - 1, json.len() - 10] {
            assert_eq!(
                document.to_json_with_limit(limit).unwrap_err().code,
                DocumentErrorCode::LimitExceeded
            );
        }
    }
}

pub(crate) fn validate_label(label: &str) -> Result<(), DocumentError> {
    if label.len() > 1024 || label.contains('\0') {
        return Err(DocumentError::new(
            DocumentErrorCode::LimitExceeded,
            "labels must contain at most 1024 UTF-8 bytes and no NUL",
        ));
    }
    Ok(())
}

fn positive(duration: FrameDuration, subject: &str) -> Result<(), DocumentError> {
    if duration == FrameDuration::ZERO {
        return Err(DocumentError::new(
            DocumentErrorCode::InvalidDuration,
            format!("{subject} duration must be positive"),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum DocumentErrorCode {
    InvalidIdentity,
    InvalidPresentation,
    InvalidAsset,
    InvalidRoot,
    InvalidTree,
    MissingNode,
    MissingAsset,
    SourceRangeInvalid,
    InvalidDuration,
    TimingOverflow,
    LimitExceeded,
    UnsupportedSchema,
    InvalidJson,
    InvalidAnchor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocumentError {
    pub code: DocumentErrorCode,
    pub message: String,
}

impl DocumentError {
    pub(crate) fn new(code: DocumentErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
    pub(crate) fn json(error: serde_json::Error) -> Self {
        Self::new(DocumentErrorCode::InvalidJson, error.to_string())
    }
}
impl fmt::Display for DocumentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl Error for DocumentError {}
impl From<TimeError> for DocumentError {
    fn from(value: TimeError) -> Self {
        Self::new(
            if value == TimeError::Overflow {
                DocumentErrorCode::TimingOverflow
            } else {
                DocumentErrorCode::InvalidDuration
            },
            value.to_string(),
        )
    }
}

/// serde's BTreeMap accepts duplicate JSON keys; authored identities must not.
pub(crate) fn unique_map<'de, D, K, V>(deserializer: D) -> Result<BTreeMap<K, V>, D::Error>
where
    D: Deserializer<'de>,
    K: Deserialize<'de> + Ord,
    V: Deserialize<'de>,
{
    struct Visitor<K, V>(std::marker::PhantomData<(K, V)>);
    impl<'de, K: Deserialize<'de> + Ord, V: Deserialize<'de>> de::Visitor<'de> for Visitor<K, V> {
        type Value = BTreeMap<K, V>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("an object with unique identity keys")
        }
        fn visit_map<A: de::MapAccess<'de>>(self, mut access: A) -> Result<Self::Value, A::Error> {
            let mut values = BTreeMap::new();
            while let Some((key, value)) = access.next_entry()? {
                if values.insert(key, value).is_some() {
                    return Err(de::Error::custom("duplicate identity key"));
                }
                if values.len() > MAX_DOCUMENT_NODES.max(MAX_DOCUMENT_ASSETS) {
                    return Err(de::Error::custom("identity map exceeds 100,000 entries"));
                }
            }
            Ok(values)
        }
    }
    deserializer.deserialize_map(Visitor(std::marker::PhantomData))
}
