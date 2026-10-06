use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::{error::Error, fmt};

use serde::{Deserialize, Deserializer, Serialize, de};

use crate::{
    AudioSample, BasisState, FrameDuration, FrameRange, FrameRate, IterationOrder, Mark,
    PlayOverrides, RepeatLayout, SourceAudioMapping, SourceEditWindow, SourceTimestamp,
    SourceVideoMapping, TimeError,
};

pub const DOCUMENT_SCHEMA_VERSION: u32 = 46;
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
identifier!(SoundId);
identifier!(TargetId);

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
    /// Exact selected editorial interval in this physical owner's local clock,
    /// after the independent audio offset. Selected audio mapping support is
    /// stored before that offset.
    /// Absence denotes generic intent without a declared common window. Stream
    /// mappings still control rendering; this interval grants no media authority.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edit_window: Option<SourceEditWindow>,
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
    /// Plays an Original picture span backwards at its natural rate, starting
    /// from the span's end. Local frame `k` shows the picture at
    /// `span.end - (k + 1/2)` project frames; past the span's start the first
    /// picture holds. No new media exists: the span is the measured Original.
    Reverse {
        asset: AssetId,
        span: SourceSpan,
    },
    /// Plays an Original picture span forward at its natural rate from its
    /// start, holding its last picture past its end: the picture of a bleep,
    /// whose own sound is replaced.
    Play {
        asset: AssetId,
        span: SourceSpan,
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
    /// A hanging effect tail: the wet output of `effect` fed with the two
    /// seconds of processed Original sound heard just before this Hold
    /// occurrence in the current edit (after time mapping, edges, gain and
    /// mute), heard from the moment that sound ends. A live reference, not a
    /// captured recording: edits before the pause change what rings. It rings
    /// for `maximum` frames, fades to exact digital silence and stays silent
    /// for the rest of the Hold; the dry sound itself is never repeated.
    Tail {
        maximum: FrameDuration,
        #[serde(default, skip_serializing_if = "TailEffect::is_reverb")]
        effect: TailEffect,
    },
    /// Plays the source audio backwards at its natural rate from the Hold's
    /// start: Hold-local sample `n` hears the source sample `n` before its
    /// span end. Silence follows once the span is exhausted.
    Reverse {
        source: SourceAudio,
    },
    /// A synthesized sine tone for the whole Hold (a bleep), with 2 ms linear
    /// ramps at both ends. Reads no media.
    Tone {
        frequency_hz: u32,
        level: crate::GainDb,
    },
}

/// Hold effects prepare their whole input in one bounded block.
pub const MAX_HOLD_EFFECT_INPUT_SAMPLES: i128 = 1_048_576;

/// Bleep tones are audible-range sines at or below full scale.
pub const TONE_FREQUENCY_HZ: std::ops::RangeInclusive<u32> = 20..=20_000;

/// The fixed, versioned effect that produces a hanging tail. Parameters are
/// part of the effect identity so preview and export render the same bytes.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum TailEffect {
    /// A dense deterministic room reverb (per-channel comb/all-pass network;
    /// its impulse response falls 60 dB in about 1.1 s).
    #[default]
    Reverb,
    /// A feedback echo of 300 ms repeats, each 6 dB quieter.
    Delay,
}

impl TailEffect {
    pub fn is_reverb(&self) -> bool {
        *self == Self::Reverb
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Reverb => "reverb",
            Self::Delay => "delay",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HoldRecipe {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub picture_context: Option<crate::CapturedFraming>,
    pub duration: FrameDuration,
    pub video: HoldVideo,
    pub audio: HoldAudio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PitchPolicy {
    Preserve,
    FollowSpeed,
    /// Pitch-preserving processing with a fixed shift of whole semitones,
    /// independent of the duration (specification §8.3 "Pitch shift"). A
    /// unity-rate Retime with a shift still runs the processor. Nonzero,
    /// within ±24 like the qualified DSP adapter.
    Shift {
        semitones: i8,
    },
}

/// The fixed pitch shift range of the canonical time/pitch processor.
pub const MAX_PITCH_SHIFT_SEMITONES: i8 = 24;

impl PitchPolicy {
    /// Pitch-preserving time/pitch processing, with or without a shift.
    pub const fn preserves(self) -> bool {
        matches!(self, Self::Preserve | Self::Shift { .. })
    }

    /// The fixed shift in semitones; zero for every other policy.
    pub const fn semitones(self) -> i8 {
        match self {
            Self::Shift { semitones } => semitones,
            Self::Preserve | Self::FollowSpeed => 0,
        }
    }

    /// Whether a Retime with this policy runs the time/pitch processor:
    /// pitch-preserving with a nonunity rate or a nonzero shift. Every other
    /// Retime maps samples transparently or as tape speed.
    pub const fn processes(self, unity_rate: bool) -> bool {
        self.preserves() && (!unity_rate || self.semitones() != 0)
    }

    /// A shift of `semitones` on pitch-preserving processing; zero is plain
    /// Preserve.
    pub fn shifted(semitones: i8) -> Result<Self, DocumentError> {
        match semitones {
            0 => Ok(Self::Preserve),
            value if value.unsigned_abs() <= MAX_PITCH_SHIFT_SEMITONES.unsigned_abs() => {
                Ok(Self::Shift { semitones: value })
            }
            _ => Err(DocumentError::new(
                DocumentErrorCode::InvalidTree,
                "a pitch shift is a nonzero whole number of semitones within ±24",
            )),
        }
    }
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
#[expect(
    clippy::large_enum_variant,
    reason = "Keep exact Source recipes inline instead of adding an allocation per video beat"
)]
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
        /// Per-play gain and scale progression; see [`crate::RepeatEscalation`].
        #[serde(default, skip_serializing_if = "Option::is_none")]
        escalation: Option<crate::RepeatEscalation>,
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
    #[serde(default, skip_serializing_if = "crate::AudioTreatments::is_empty")]
    pub audio_treatments: crate::AudioTreatments,
    #[serde(
        default,
        skip_serializing_if = "crate::AudioEdgePolicies::is_automatic"
    )]
    pub audio_edges: crate::AudioEdgePolicies,
    #[serde(default, skip_serializing_if = "crate::AudioEditorialEdges::is_empty")]
    pub audio_editorial_edges: crate::AudioEditorialEdges,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub framing: Option<crate::Framing>,
    /// Picture-only attachments in this beat's local clock.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cutaways: Vec<crate::Cutaway>,
    /// Text drawn over the picture in this beat's local clock.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub captions: Vec<crate::Caption>,
}

impl BeatNode {
    pub fn sequence(label: impl Into<String>, children: Vec<NodeId>) -> Self {
        Self {
            label: label.into(),
            kind: NodeKind::Sequence { children },
            audio_treatments: Default::default(),
            audio_edges: crate::AudioEdgePolicies::default(),
            audio_editorial_edges: Default::default(),
            framing: None,
            cutaways: Vec::new(),
            captions: Vec::new(),
        }
    }
    pub fn hold(label: impl Into<String>, recipe: HoldRecipe) -> Self {
        Self {
            label: label.into(),
            kind: NodeKind::Hold { recipe },
            audio_treatments: Default::default(),
            audio_edges: crate::AudioEdgePolicies::default(),
            audio_editorial_edges: Default::default(),
            framing: None,
            cutaways: Vec::new(),
            captions: Vec::new(),
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
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) sounds: BTreeMap<SoundId, crate::SoundEvent>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) beat_sounds: BTreeMap<NodeId, BTreeMap<SoundId, crate::BeatSound>>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) sound_routes: BTreeMap<SoundId, crate::RootSoundRoute>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) sound_allowances: BTreeMap<SoundId, crate::SoundHoldAllowances>,
    pub(crate) overrides: BTreeMap<NodeId, PlayOverrides>,
    /// Independently owned gap subtrees, keyed by the stable preceding play.
    /// A final play retains its override without rendering trailing time.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) gap_overrides: BTreeMap<NodeId, PlayOverrides>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) audio_lineage: BTreeMap<NodeId, crate::AudioLineageId>,
    #[serde(skip_serializing_if = "crate::AudioBindingState::is_empty")]
    pub(crate) audio_bindings: crate::AudioBindingState,
    /// Attention targets followed in source time.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) targets: BTreeMap<TargetId, crate::AttentionTarget>,
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
    #[serde(deserialize_with = "crate::audio_gain::node_map")]
    nodes: BTreeMap<NodeId, BeatNode>,
    #[serde(deserialize_with = "unique_map")]
    assets: BTreeMap<AssetId, AssetRecord>,
    #[serde(deserialize_with = "unique_map")]
    marks: BTreeMap<MarkId, Mark>,
    #[serde(default, deserialize_with = "unique_map")]
    sounds: BTreeMap<SoundId, crate::SoundEvent>,
    #[serde(default, deserialize_with = "crate::sound_events::beat_sounds_map")]
    beat_sounds: BTreeMap<NodeId, BTreeMap<SoundId, crate::BeatSound>>,
    #[serde(default, deserialize_with = "unique_map")]
    sound_routes: BTreeMap<SoundId, crate::RootSoundRoute>,
    #[serde(default, deserialize_with = "unique_map")]
    sound_allowances: BTreeMap<SoundId, crate::SoundHoldAllowances>,
    #[serde(deserialize_with = "unique_map")]
    overrides: BTreeMap<NodeId, PlayOverrides>,
    #[serde(default, deserialize_with = "unique_map")]
    gap_overrides: BTreeMap<NodeId, PlayOverrides>,
    #[serde(default, deserialize_with = "unique_map")]
    audio_lineage: BTreeMap<NodeId, crate::AudioLineageId>,
    #[serde(default)]
    audio_bindings: crate::AudioBindingState,
    #[serde(default, deserialize_with = "unique_map")]
    targets: BTreeMap<TargetId, crate::AttentionTarget>,
}

impl TryFrom<DocumentWire> for ProjectDocument {
    type Error = DocumentError;
    fn try_from(value: DocumentWire) -> Result<Self, Self::Error> {
        let document = Self::from_wire(value);
        document.validate()?;
        Ok(document)
    }
}

impl ProjectDocument {
    /// Unvalidated; every caller validates before exposing the document.
    fn from_wire(value: DocumentWire) -> Self {
        Self {
            schema_version: value.schema_version,
            project_id: value.project_id,
            revision_id: value.revision_id,
            presentation_basis: value.presentation_basis,
            basis_state: value.basis_state,
            root: value.root,
            nodes: value.nodes,
            assets: value.assets,
            marks: value.marks,
            sounds: value.sounds,
            beat_sounds: value.beat_sounds,
            sound_routes: value.sound_routes,
            sound_allowances: value.sound_allowances,
            overrides: value.overrides,
            gap_overrides: value.gap_overrides,
            audio_lineage: value.audio_lineage,
            audio_bindings: value.audio_bindings,
            targets: value.targets,
        }
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
            sounds: BTreeMap::new(),
            beat_sounds: BTreeMap::new(),
            sound_routes: BTreeMap::new(),
            sound_allowances: BTreeMap::new(),
            overrides: BTreeMap::new(),
            gap_overrides: BTreeMap::new(),
            audio_lineage: BTreeMap::new(),
            audio_bindings: crate::AudioBindingState::default(),
            targets: BTreeMap::new(),
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
    /// Build a detached, validated view of one captured immutable source.
    /// The project, revision and presentation basis are retained, but authored
    /// edits, bindings, marks and framing are not copied into this view. This
    /// constructs no edit transaction and never changes the captured document.
    pub fn source_view(
        &self,
        asset: &AssetId,
        source: SourceNode,
        root: NodeId,
        node: NodeId,
    ) -> Result<Self, DocumentError> {
        if root == node {
            return Err(DocumentError::new(
                DocumentErrorCode::InvalidTree,
                "source view root and source identities must differ",
            ));
        }
        let record = self.assets.get(asset).ok_or_else(|| {
            DocumentError::new(
                DocumentErrorCode::MissingAsset,
                "source view asset is absent",
            )
        })?;
        let mut view = Self::new(
            self.project_id.clone(),
            self.revision_id.clone(),
            self.presentation_basis.clone(),
            root.clone(),
        )?;
        view.assets.insert(asset.clone(), record.clone());
        view.nodes
            .insert(root, BeatNode::sequence("Original", vec![node.clone()]));
        view.nodes.insert(
            node,
            BeatNode {
                audio_treatments: Default::default(),
                label: record.label.clone(),
                framing: None,
                audio_editorial_edges: Default::default(),
                audio_edges: Default::default(),
                kind: NodeKind::Source { source },
                cutaways: Vec::new(),
                captions: Vec::new(),
            },
        );
        view.validate()?;
        Ok(view)
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
    pub fn sound_routes(&self) -> &BTreeMap<SoundId, crate::RootSoundRoute> {
        &self.sound_routes
    }

    pub fn sounds(&self) -> &BTreeMap<SoundId, crate::SoundEvent> {
        &self.sounds
    }
    pub fn beat_sounds(&self) -> &BTreeMap<NodeId, BTreeMap<SoundId, crate::BeatSound>> {
        &self.beat_sounds
    }
    pub fn targets(&self) -> &BTreeMap<TargetId, crate::AttentionTarget> {
        &self.targets
    }

    pub fn sound_allowances(&self) -> &BTreeMap<SoundId, crate::SoundHoldAllowances> {
        &self.sound_allowances
    }
    pub fn overrides(&self) -> &BTreeMap<NodeId, PlayOverrides> {
        &self.overrides
    }
    pub fn gap_overrides(&self) -> &BTreeMap<NodeId, PlayOverrides> {
        &self.gap_overrides
    }
    pub fn audio_lineage(&self) -> &BTreeMap<NodeId, crate::AudioLineageId> {
        &self.audio_lineage
    }
    pub fn audio_bindings(&self) -> &crate::AudioBindingState {
        &self.audio_bindings
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
            .chain(
                self.gap_overrides
                    .get(id)
                    .into_iter()
                    .flat_map(|entries| entries.iter().map(|(_, root)| root)),
            )
    }

    pub fn to_json(&self) -> Result<String, DocumentError> {
        self.to_json_with_limit(MAX_DOCUMENT_JSON_BYTES)
    }

    /// The same document without indentation, for durable storage. Pretty
    /// output roughly doubles every stored revision; readers accept both.
    pub fn to_compact_json(&self) -> Result<String, DocumentError> {
        let mut output = BoundedJson {
            bytes: Vec::new(),
            limit: MAX_DOCUMENT_JSON_BYTES,
            exceeded: false,
        };
        let result = serde_json::to_writer(&mut output, self);
        if output.exceeded {
            return Err(json_limit());
        }
        result.map_err(DocumentError::json)?;
        String::from_utf8(output.bytes)
            .map_err(|error| DocumentError::new(DocumentErrorCode::InvalidJson, error.to_string()))
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
        crate::framing::preflight(json)?;
        crate::picture_context::preflight(json)?;
        let wire: DocumentWire = serde_json::from_str(json).map_err(DocumentError::json)?;
        Self::try_from(wire)
    }

    pub fn validate(&self) -> Result<(), DocumentError> {
        if validated::durations(self).is_some() {
            return Ok(());
        }
        self.durations().map(|_| ())
    }
    pub fn duration(&self) -> Result<FrameDuration, DocumentError> {
        self.node_duration(&self.root)
    }
    /// The root duration from the structural pass alone. For a validated
    /// document this equals [`Self::duration`] without repeating the
    /// document-wide binding, mark and effect checks.
    pub fn structural_duration(&self) -> Result<FrameDuration, DocumentError> {
        self.structural_durations()?
            .get(&self.root)
            .copied()
            .ok_or_else(|| {
                DocumentError::new(DocumentErrorCode::MissingNode, "root does not exist")
            })
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
        if let Some(durations) = validated::durations(self) {
            return Ok(durations.as_ref().clone());
        }
        self.durations_with_context_limits(
            crate::MAX_CAPTURED_FRAMING_RECORDS,
            crate::MAX_GAIN_RECORDS,
        )
    }

    /// Private intermediate state only: the pending edit can retire copied
    /// context. Public validation still enforces the ordinary document budget.
    pub(crate) fn validate_isolated_context(&self) -> Result<(), DocumentError> {
        self.durations_with_context_limits(
            crate::picture_context::MAX_ISOLATED_FRAMING_RECORDS,
            crate::audio_gain::MAX_ISOLATED_GAIN_RECORDS,
        )
        .map(|_| ())
    }

    fn durations_with_context_limits(
        &self,
        context_limit: usize,
        gain_limit: usize,
    ) -> Result<BTreeMap<NodeId, FrameDuration>, DocumentError> {
        self.validated_with_context_limits(context_limit, gain_limit, None, None)
            .map(|(durations, _)| durations)
    }

    /// Complete validation of this document. With `previous`, the audio
    /// binding owners that the patch left unchanged reuse their proved
    /// placement checks (see `AudioBindingState::validation_work`); every
    /// other invariant is checked over the complete document.
    /// Complete validation, retaining the per-owner binding proof.
    pub(crate) fn validated_with_proof(
        &self,
    ) -> Result<
        (
            BTreeMap<NodeId, FrameDuration>,
            crate::audio_binding::BindingProof,
        ),
        DocumentError,
    > {
        self.validated_with_structure(None)
    }

    /// Complete validation, given this document's structural durations when
    /// a caller already computed them for exactly these nodes, overrides,
    /// assets, basis and root.
    pub(crate) fn validated_with_structure(
        &self,
        structure: Option<crate::command::SharedDurations>,
    ) -> Result<
        (
            BTreeMap<NodeId, FrameDuration>,
            crate::audio_binding::BindingProof,
        ),
        DocumentError,
    > {
        self.validated_with_context_limits(
            crate::MAX_CAPTURED_FRAMING_RECORDS,
            crate::MAX_GAIN_RECORDS,
            None,
            structure,
        )
    }

    pub(crate) fn validate_after(
        &self,
        previous: &ValidatedDocument,
        bindings: Option<&crate::AudioBindingPatch>,
    ) -> Result<
        (
            BTreeMap<NodeId, FrameDuration>,
            crate::audio_binding::BindingProof,
        ),
        DocumentError,
    > {
        self.validate_after_with(previous, bindings, None)
    }

    /// [`Self::validate_after`], given known structural durations as in
    /// [`Self::validated_with_structure`].
    pub(crate) fn validate_after_with(
        &self,
        previous: &ValidatedDocument,
        bindings: Option<&crate::AudioBindingPatch>,
        structure: Option<crate::command::SharedDurations>,
    ) -> Result<
        (
            BTreeMap<NodeId, FrameDuration>,
            crate::audio_binding::BindingProof,
        ),
        DocumentError,
    > {
        self.validated_with_context_limits(
            crate::MAX_CAPTURED_FRAMING_RECORDS,
            crate::MAX_GAIN_RECORDS,
            Some((&previous.bindings, bindings)),
            structure,
        )
    }

    fn validated_with_context_limits(
        &self,
        context_limit: usize,
        gain_limit: usize,
        previous: Option<(
            &crate::audio_binding::BindingProof,
            Option<&crate::AudioBindingPatch>,
        )>,
        structure: Option<crate::command::SharedDurations>,
    ) -> Result<
        (
            BTreeMap<NodeId, FrameDuration>,
            crate::audio_binding::BindingProof,
        ),
        DocumentError,
    > {
        // The structural pass is a pure function of the nodes, overrides,
        // assets, basis and root; a caller that computed it for exactly
        // these hands over its result.
        let durations = match structure {
            Some(structure) => {
                #[cfg(debug_assertions)]
                assert!(
                    self.structural_durations().as_ref() == Ok(&*structure),
                    "given structural durations describe another structure"
                );
                Arc::try_unwrap(structure).unwrap_or_else(|shared| (*shared).clone())
            }
            None => self.structural_durations()?,
        };
        crate::sound_events::validate(self, &durations)?;
        crate::sound_allowance::validate(self)?;
        crate::framing::validate_document(self)?;
        for node in self.nodes.values() {
            if !(node.cutaways.is_empty() && node.captions.is_empty())
                && !matches!(node.kind, NodeKind::Source { .. } | NodeKind::Hold { .. })
            {
                return Err(DocumentError::new(
                    DocumentErrorCode::InvalidTree,
                    "cutaways and captions belong to a Source or Hold beat, whose local clock is its content",
                ));
            }
            crate::cutaway::validate(&node.cutaways, &self.assets)?;
            crate::caption::validate(&node.captions)?;
        }
        self.validate_tails_outside_speed_stages()?;
        crate::target::validate_targets(&self.targets, &self.assets)?;
        crate::audio_gain::validate_document(self, gain_limit)?;
        crate::picture_context::validate_nodes_with_limit(self.nodes.values(), context_limit)?;
        self.validate_basis_state(&durations)?;
        crate::audio_lineage::validate(self)?;
        let bindings = self.audio_bindings.validate_for_after(self, previous)?;
        crate::marks::validate_marks(self, &durations)?;
        Ok((durations, bindings))
    }

    /// A hanging tail is fed by what is heard before it on the edit clock.
    /// Inside a speed change (a nonunity Retime, either pitch policy) that
    /// clock does not exist and the ring would be stretched or pitch-shifted,
    /// so a tail Hold or Repeat gap there is invalid. Transparent Partitions
    /// and unity Retimes keep the edit clock.
    fn validate_tails_outside_speed_stages(&self) -> Result<(), DocumentError> {
        let speed_stage = |kind: &NodeKind| {
            matches!(
                kind,
                NodeKind::Retime { duration, mapping, purpose, pitch, .. }
                    if purpose.is_edit()
                        && (*duration != mapping.duration() || pitch.semitones() != 0)
            )
        };
        // Only a node below a speed stage can carry a tail into it.
        if !self.nodes.values().any(|node| speed_stage(&node.kind)) {
            return Ok(());
        }
        let tail = |audio: &HoldAudio| matches!(audio, HoldAudio::Tail { .. });
        let mut stack = vec![(&self.root, false)];
        while let Some((id, inside)) = stack.pop() {
            let Some(node) = self.nodes.get(id) else {
                continue;
            };
            let carried = match &node.kind {
                NodeKind::Hold { recipe } => inside && tail(&recipe.audio),
                NodeKind::Repeat { gap, .. } => {
                    inside && gap.as_ref().is_some_and(|gap| tail(&gap.audio))
                }
                _ => false,
            };
            if carried {
                return Err(DocumentError::new(
                    DocumentErrorCode::InvalidTree,
                    format!(
                        "a hanging tail cannot sit inside a speed change (at {id}); put the pause outside the Retime"
                    ),
                ));
            }
            let speed = speed_stage(&node.kind);
            for child in self.children(id) {
                stack.push((child, inside || speed));
            }
        }
        Ok(())
    }

    /// Structural validation precedes mark transforms; no anchor validation
    /// calls back into this traversal or into public `durations()`.
    pub(crate) fn structural_durations(
        &self,
    ) -> Result<BTreeMap<NodeId, FrameDuration>, DocumentError> {
        // Complete validation returns exactly the structural durations.
        if let Some(durations) = validated::durations(self) {
            return Ok(durations.as_ref().clone());
        }
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
        if self.overrides.len() > MAX_DOCUMENT_NODES
            || self.gap_overrides.len() > MAX_DOCUMENT_NODES
        {
            return Err(DocumentError::new(
                DocumentErrorCode::LimitExceeded,
                "too many override owners",
            ));
        }
        for (id, entries) in self.overrides.iter().chain(&self.gap_overrides) {
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
        // Every structural reference: primitive children plus override roots.
        // Override owners were checked to be existing nodes above.
        let mut edge_count = 0usize;
        for count in self
            .nodes
            .values()
            .map(|node| node.kind.children().len())
            .chain(
                self.overrides
                    .values()
                    .chain(self.gap_overrides.values())
                    .map(|entries| entries.iter().count()),
            )
        {
            edge_count = edge_count.checked_add(count).ok_or_else(|| {
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
            Self::validate_asset(id, asset)?;
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
        // Borrow identities during the walk; only the returned map owns them.
        let mut seen = BTreeSet::new();
        let mut stack: Vec<(&NodeId, usize, Option<&BeatNode>)> = vec![(&self.root, 0usize, None)];
        let mut durations = BTreeMap::new();
        while let Some((id, depth, visited)) = stack.pop() {
            if depth > MAX_DOCUMENT_DEPTH {
                return Err(DocumentError::new(
                    DocumentErrorCode::LimitExceeded,
                    "document structural depth exceeds 256 edges",
                ));
            }
            let Some(node) = visited else {
                let node = self.nodes.get(id).ok_or_else(|| {
                    DocumentError::new(
                        DocumentErrorCode::MissingNode,
                        format!("node {id} does not exist"),
                    )
                })?;
                if !seen.insert(id) {
                    return Err(DocumentError::new(
                        DocumentErrorCode::InvalidTree,
                        format!("node {id} has multiple parents or forms a cycle"),
                    ));
                }
                validate_label(&node.label)?;
                node.audio_edges.validate(&node.kind)?;
                stack.push((id, depth, Some(node)));
                // The same order as `children`: primitive, override, gap roots.
                let overrides = self
                    .overrides
                    .get(id)
                    .into_iter()
                    .flat_map(|entries| entries.iter().map(|(_, root)| root));
                let gaps = self
                    .gap_overrides
                    .get(id)
                    .into_iter()
                    .flat_map(|entries| entries.iter().map(|(_, root)| root));
                let children: Vec<&NodeId> = node
                    .kind
                    .children()
                    .iter()
                    .chain(overrides)
                    .chain(gaps)
                    .collect();
                for child in children.into_iter().rev() {
                    stack.push((child, depth + 1, None));
                }
                continue;
            };
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
                    if let Some(window) = source.edit_window {
                        window.validate(source.duration)?;
                    }
                    match &source.video {
                        SourceVideo::Stream { asset, span } => {
                            self.validate_video_span(asset, *span)?;
                            source
                                .video_mapping
                                .selection_in_source(*span, source.duration)?;
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
                        if matches!(
                            source.audio_mapping,
                            SourceAudioMapping::Placement { .. }
                                | SourceAudioMapping::SelectedPlacement { .. }
                        ) {
                            source
                                .audio_mapping
                                .start_frames_with_offset(
                                    source.audio_offset,
                                    self.presentation_basis.frame_rate,
                                )?
                                .checked_add(frames)?;
                        }
                        source.audio_mapping.selection_frames_with_offset(
                            source.duration,
                            source.audio_offset,
                            self.presentation_basis.frame_rate,
                        )?;
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
                    escalation,
                } => {
                    iterations.validate()?;
                    if let Some(escalation) = escalation {
                        let plays = iterations.len();
                        escalation.validate(plays).map_err(|error| {
                            DocumentError::new(
                                DocumentErrorCode::InvalidTree,
                                format!("Repeat escalation: {error}"),
                            )
                        })?;
                    }
                    if let Some(gap) = gap {
                        self.validate_hold(gap)?;
                    }
                    RepeatLayout::compile_with_gap_overrides(
                        iterations,
                        child,
                        self.overrides.get(id),
                        gap.as_ref().map_or(FrameDuration::ZERO, |gap| gap.duration),
                        self.gap_overrides.get(id),
                        &durations,
                    )?
                    .duration()
                }
                NodeKind::Retime {
                    child,
                    duration,
                    mapping,
                    purpose,
                    pitch,
                } => {
                    positive(*duration, "retime")?;
                    if let PitchPolicy::Shift { semitones } = pitch
                        && (PitchPolicy::shifted(*semitones)? != *pitch
                            || *purpose == RetimePurpose::Partition)
                    {
                        return Err(DocumentError::new(
                            DocumentErrorCode::InvalidTree,
                            "a pitch shift is a nonzero whole number of semitones within ±24 on an ordinary Retime",
                        ));
                    }
                    if *purpose == RetimePurpose::Partition
                        && (*duration != mapping.duration()
                            || !node
                                .audio_editorial_edges
                                .permits_partition_policies(node.audio_edges))
                    {
                        return Err(DocumentError::new(
                            DocumentErrorCode::InvalidTree,
                            "a partition requires unity timing and Hard policies only on marked editorial sides",
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
            durations.insert(id.clone(), duration);
        }
        if seen.len() != self.nodes.len() {
            return Err(DocumentError::new(
                DocumentErrorCode::InvalidTree,
                "document contains nodes unreachable from its root",
            ));
        }
        Ok(durations)
    }

    pub(crate) fn validate_asset(id: &AssetId, asset: &AssetRecord) -> Result<(), DocumentError> {
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
    pub(crate) fn validate_hold(&self, recipe: &HoldRecipe) -> Result<(), DocumentError> {
        positive(recipe.duration, "hold")?;
        if let Some(context) = &recipe.picture_context {
            context.validate()?;
        }
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
            HoldVideo::Reverse { asset, span } | HoldVideo::Play { asset, span } => {
                self.validate_video_span(asset, *span)?
            }
        }
        match &recipe.audio {
            HoldAudio::Silence => {}
            HoldAudio::Tone {
                frequency_hz,
                level,
            } => {
                if !TONE_FREQUENCY_HZ.contains(frequency_hz) || level.millidecibels() > 0 {
                    return Err(DocumentError::new(
                        DocumentErrorCode::SourceRangeInvalid,
                        "a tone is 20 Hz to 20 kHz at or below full scale",
                    ));
                }
            }
            HoldAudio::RoomTone { source } => self.validate_audio(source)?,
            HoldAudio::Reverse { source } => {
                self.validate_audio(source)?;
                // The reversal reads its whole span in one bounded block.
                let base = source.span.start().time_base;
                let seconds = crate::ExactRatio::new(
                    i128::from(source.span.end().ticks - source.span.start().ticks)
                        * i128::from(base.numerator()),
                    i128::from(base.denominator()),
                )
                .map_err(|_| {
                    DocumentError::new(DocumentErrorCode::SourceRangeInvalid, "reverse span")
                })?;
                if seconds
                    .checked_mul(crate::ExactRatio::integer(i64::from(
                        crate::MIX_SAMPLE_RATE,
                    )))
                    .ok()
                    .and_then(|samples| samples.ceil().ok())
                    .is_none_or(|samples| samples > MAX_HOLD_EFFECT_INPUT_SAMPLES)
                {
                    return Err(DocumentError::new(
                        DocumentErrorCode::SourceRangeInvalid,
                        "a reversed span is at most 1,048,576 samples at 48 kHz (about 21.8 s)",
                    ));
                }
            }
            HoldAudio::Tail { maximum, .. } => {
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
        if artifact
            .content_aspect
            .is_some_and(|[width, height]| width == 0 || height == 0)
        {
            return Err(DocumentError::new(
                DocumentErrorCode::InvalidTree,
                "a generated Hold's content aspect has positive dimensions",
            ));
        }
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

/// A document proved valid, with the durations its validation computed.
///
/// The document is shared and never mutably reachable, so the proof cannot be
/// invalidated. Inside [`ValidatedDocument::scope`], validation queries on this
/// exact document return the retained result instead of repeating the
/// whole-document traversal: a pure function memoized on an immutable value.
#[derive(Debug, Clone)]
pub struct ValidatedDocument {
    document: Arc<ProjectDocument>,
    durations: Arc<BTreeMap<NodeId, FrameDuration>>,
    /// Retained per-owner audio binding work, for the next validation.
    bindings: Arc<crate::audio_binding::BindingProof>,
}

impl ValidatedDocument {
    /// Validate a document once.
    pub fn new(document: Arc<ProjectDocument>) -> Result<Self, DocumentError> {
        let (durations, bindings) = document.validated_with_context_limits(
            crate::MAX_CAPTURED_FRAMING_RECORDS,
            crate::MAX_GAIN_RECORDS,
            None,
            None,
        )?;
        Ok(Self {
            document,
            durations: Arc::new(durations),
            bindings: Arc::new(bindings),
        })
    }

    /// A result whose complete validation produced `durations` and `bindings`.
    pub(crate) fn from_validation(
        document: ProjectDocument,
        durations: BTreeMap<NodeId, FrameDuration>,
        bindings: crate::audio_binding::BindingProof,
    ) -> Self {
        Self {
            document: Arc::new(document),
            durations: Arc::new(durations),
            bindings: Arc::new(bindings),
        }
    }

    pub fn document(&self) -> &Arc<ProjectDocument> {
        &self.document
    }

    /// Apply a guarded patch and validate the result once. A stored patch
    /// names every binding owner and timing table it changes, so unchanged
    /// owners reuse their proved checks (see `AudioBindingState`).
    pub fn apply_patch(&self, patch: &crate::DocumentPatch) -> Result<Self, crate::EditError> {
        let next = patch.apply_stored(&self.document)?;
        let (durations, bindings) = next.validate_after(self, patch.audio_bindings.as_ref())?;
        Ok(Self::from_validation(next, durations, bindings))
    }

    /// Recompute this document's validation from scratch, outside any
    /// retained scope, and require exactly the retained result. Hosts call
    /// this in debug builds to check that reused validation matched the
    /// complete one.
    pub fn check_against_complete_validation(&self) -> Result<(), DocumentError> {
        let (durations, bindings) = self.document.validated_with_context_limits(
            crate::MAX_CAPTURED_FRAMING_RECORDS,
            crate::MAX_GAIN_RECORDS,
            None,
            None,
        )?;
        if durations != *self.durations || bindings != *self.bindings {
            return Err(DocumentError::new(
                DocumentErrorCode::InvalidTree,
                "retained validation differs from complete validation",
            ));
        }
        Ok(())
    }

    pub(crate) fn binding_proof(&self) -> &crate::audio_binding::BindingProof {
        &self.bindings
    }

    pub(crate) fn into_document(self) -> Arc<ProjectDocument> {
        self.document
    }

    pub fn durations(&self) -> &BTreeMap<NodeId, FrameDuration> {
        &self.durations
    }

    /// Run `f` with this document's validation retained for queries on it.
    pub fn scope<R>(&self, f: impl FnOnce() -> R) -> R {
        let _guard = validated::enter(&self.document, &self.durations, &self.bindings);
        f()
    }
}

impl ProjectDocument {
    /// Complete validation of an intermediate edit result. Inside a validated
    /// document's scope, binding owners unchanged from that document reuse
    /// its proof exactly as a committed result does (`validate_after`): the
    /// reused checks are pure functions of the binding and the immutable
    /// tables it names, so the outcome equals [`Self::validated_with_proof`].
    pub(crate) fn validated_in_scope(
        &self,
    ) -> Result<
        (
            BTreeMap<NodeId, FrameDuration>,
            crate::audio_binding::BindingProof,
        ),
        DocumentError,
    > {
        match validated::innermost_proof().filter(|_| crate::command_work::local()) {
            Some((head, proof)) => {
                let patch =
                    crate::AudioBindingPatch::between(&head.audio_bindings, &self.audio_bindings);
                self.validated_with_context_limits(
                    crate::MAX_CAPTURED_FRAMING_RECORDS,
                    crate::MAX_GAIN_RECORDS,
                    Some((&proof, patch.as_ref())),
                    None,
                )
            }
            None => self.validated_with_proof(),
        }
    }

    /// `state.validate_for(self)`, reusing the scoped proof as
    /// [`Self::validated_in_scope`] does.
    pub(crate) fn validate_bindings_in_scope(
        &self,
        state: &crate::AudioBindingState,
    ) -> Result<(), DocumentError> {
        match validated::innermost_proof().filter(|_| crate::command_work::local()) {
            Some((head, proof)) => {
                let patch = crate::AudioBindingPatch::between(&head.audio_bindings, state);
                state
                    .validate_for_after(self, Some((&proof, patch.as_ref())))
                    .map(|_| ())
            }
            None => state.validate_for(self),
        }
    }

    /// The durations retained for this exact document by an enclosing
    /// validation scope, shared rather than copied.
    pub(crate) fn retained_durations(&self) -> Option<Arc<BTreeMap<NodeId, FrameDuration>>> {
        validated::durations(self)
    }

    /// Run `f` with `self` treated as validated with `durations`.
    ///
    /// Callers prove that complete validation of `self` would succeed and
    /// return exactly `durations`; the usual proof is that `self` differs
    /// from a validated document only in its audio binding state, which
    /// `AudioBindingState::validate_for` accepted against that document (no
    /// other invariant reads the binding state, and none of them reads it).
    /// The borrow keeps `self` unchanged while the scope exists.
    pub(crate) fn with_proved_durations<R>(
        &self,
        durations: Arc<BTreeMap<NodeId, FrameDuration>>,
        f: impl FnOnce() -> R,
    ) -> R {
        #[cfg(debug_assertions)]
        {
            let complete = self.durations_with_context_limits(
                crate::MAX_CAPTURED_FRAMING_RECORDS,
                crate::MAX_GAIN_RECORDS,
            );
            assert!(
                complete
                    .as_ref()
                    .is_ok_and(|complete| *complete == *durations),
                "proved durations differ from complete validation"
            );
        }
        let _guard = validated::enter_borrowed(self, durations);
        f()
    }
}

impl std::ops::Deref for ValidatedDocument {
    type Target = ProjectDocument;
    fn deref(&self) -> &ProjectDocument {
        &self.document
    }
}

impl ProjectDocument {
    /// Parse and validate once, retaining the proof.
    pub fn from_json_validated(json: &str) -> Result<ValidatedDocument, DocumentError> {
        if json.len() > MAX_DOCUMENT_JSON_BYTES {
            return Err(json_limit());
        }
        crate::framing::preflight(json)?;
        crate::picture_context::preflight(json)?;
        let wire: DocumentWire = serde_json::from_str(json).map_err(DocumentError::json)?;
        ValidatedDocument::new(Arc::new(Self::from_wire(wire)))
    }
}

impl PartialEq for ValidatedDocument {
    fn eq(&self, other: &Self) -> bool {
        self.document == other.document
    }
}

mod validated {
    use super::*;
    use std::cell::RefCell;

    /// A scoped document, its durations, and for a [`ValidatedDocument`] the
    /// shared document with its binding proof.
    type Entry = (
        *const ProjectDocument,
        Arc<BTreeMap<NodeId, FrameDuration>>,
        Option<Proved>,
    );
    pub(super) type Proved = (
        Arc<ProjectDocument>,
        Arc<crate::audio_binding::BindingProof>,
    );

    thread_local! {
        static SCOPES: RefCell<Vec<Entry>> = const { RefCell::new(Vec::new()) };
    }

    pub(super) struct Guard;

    impl Drop for Guard {
        fn drop(&mut self) {
            SCOPES.with(|scopes| {
                scopes.borrow_mut().pop();
            });
        }
    }

    /// The guard lives inside `ValidatedDocument::scope`, which borrows the
    /// shared document for the whole scope, so the address cannot be reused
    /// by another document while the entry exists.
    pub(super) fn enter(
        document: &Arc<ProjectDocument>,
        durations: &Arc<BTreeMap<NodeId, FrameDuration>>,
        bindings: &Arc<crate::audio_binding::BindingProof>,
    ) -> Guard {
        SCOPES.with(|scopes| {
            scopes.borrow_mut().push((
                Arc::as_ptr(document),
                Arc::clone(durations),
                Some((Arc::clone(document), Arc::clone(bindings))),
            ));
        });
        Guard
    }

    /// The innermost validated document in scope and its binding proof.
    pub(super) fn innermost_proof() -> Option<Proved> {
        SCOPES.with(|scopes| {
            scopes
                .borrow()
                .iter()
                .rev()
                .find_map(|(_, _, proved)| proved.clone())
        })
    }

    /// [`enter`] for a document borrowed for the guard's whole lifetime.
    pub(super) fn enter_borrowed(
        document: &ProjectDocument,
        durations: Arc<BTreeMap<NodeId, FrameDuration>>,
    ) -> Guard {
        SCOPES.with(|scopes| {
            scopes
                .borrow_mut()
                .push((std::ptr::from_ref(document), durations, None));
        });
        Guard
    }

    pub(super) fn durations(
        document: &ProjectDocument,
    ) -> Option<Arc<BTreeMap<NodeId, FrameDuration>>> {
        SCOPES.with(|scopes| {
            scopes
                .borrow()
                .iter()
                .rev()
                .find(|(pointer, _, _)| std::ptr::eq(*pointer, document))
                .map(|(_, durations, _)| Arc::clone(durations))
        })
    }
}

#[cfg(test)]
#[path = "validated_tests.rs"]
mod validated_tests;
