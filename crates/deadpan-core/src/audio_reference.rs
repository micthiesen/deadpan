//! Immutable timing and audibility facts. Aliases belong to this layout, never
//! to a later live document; no media, marks or previous bindings are retained.

mod exact_clock;
mod isolation_correspondence;
mod preflight;
pub use exact_clock::FrozenRetimeClock;
mod sound_placement;
mod split_correspondence;
pub use sound_placement::{FrozenSoundContextClock, FrozenSoundPlacement};

use crate::audio_binding::Work;
use crate::audio_binding::admission::{
    AdmittedResult, AudioBindingAdmission, Build, ProjectedSupport, Unmetered, legacy,
};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::sync::Arc;

mod explicit_edges;

use serde::{Deserialize, Serialize};

use crate::document::unique_map;
use crate::{
    AudioBoundaryKind, AudioEdgePolicies, AudioEdgePolicy, AudioLineageId, DocumentError,
    DocumentErrorCode, ExactRatio, FrameDuration, FrameRange, FrameRate, HoldAudio, InstancePath,
    IterationId, IterationOrder, MAX_DOCUMENT_DEPTH, MAX_DOCUMENT_JSON_BYTES, MAX_DOCUMENT_NODES,
    NodeId, NodeKind, PitchPolicy, PlayOverrides, ProjectDocument, RepeatLayout, RetimePurpose,
    TimeError,
};

/// Frozen references additionally bound the sum of compact runs across every
/// Repeat. Current authored documents have per-Repeat limits instead.
pub const MAX_FROZEN_AUDIO_RUNS: usize = 100_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(deny_unknown_fields)]
pub struct ExactFrameRange {
    pub start: ExactRatio,
    pub end: ExactRatio,
}

impl ExactFrameRange {
    pub fn new(start: ExactRatio, end: ExactRatio) -> Result<Self, DocumentError> {
        crate::source_mapping::validate_duration(end.checked_sub(start)?)?;
        Ok(Self { start, end })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReferenceAudibility {
    Silence,
    RoomTone,
    Tail {
        maximum: FrameDuration,
        #[serde(skip_serializing_if = "crate::TailEffect::is_reverb")]
        effect: crate::TailEffect,
        #[serde(skip_serializing_if = "crate::TailInput::is_original")]
        input: crate::TailInput,
    },
    /// Source audio played backwards from the Hold's start.
    Reverse,
    /// A synthesized tone; no source input.
    Tone {
        frequency_hz: u32,
        level: crate::GainDb,
    },
    /// The live reversed passage before the occurrence; no source input.
    Rewind {
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        bounce: bool,
    },
    /// A chosen source sound, once or looped, with explicit fades.
    Custom {
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        looped: bool,
        #[serde(skip_serializing_if = "is_zero")]
        fade_in_ms: u32,
        #[serde(skip_serializing_if = "is_zero")]
        fade_out_ms: u32,
    },
}

fn is_zero(value: &u32) -> bool {
    *value == 0
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum AudibilityWire {
    Silence {},
    RoomTone {},
    Tail {
        maximum: FrameDuration,
        #[serde(default)]
        effect: crate::TailEffect,
        #[serde(default)]
        input: crate::TailInput,
    },
    Reverse {},
    Tone {
        frequency_hz: u32,
        level: crate::GainDb,
    },
    Rewind {
        #[serde(default)]
        bounce: bool,
    },
    Custom {
        #[serde(default)]
        looped: bool,
        #[serde(default)]
        fade_in_ms: u32,
        #[serde(default)]
        fade_out_ms: u32,
    },
}

impl<'de> Deserialize<'de> for ReferenceAudibility {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let audio = match AudibilityWire::deserialize(deserializer)? {
            AudibilityWire::Silence {} => Self::Silence,
            AudibilityWire::RoomTone {} => Self::RoomTone,
            AudibilityWire::Tail {
                maximum,
                effect,
                input,
            } => Self::Tail {
                maximum,
                effect,
                input,
            },
            AudibilityWire::Reverse {} => Self::Reverse,
            AudibilityWire::Rewind { bounce } => Self::Rewind { bounce },
            AudibilityWire::Custom {
                looped,
                fade_in_ms,
                fade_out_ms,
            } => Self::Custom {
                looped,
                fade_in_ms,
                fade_out_ms,
            },
            AudibilityWire::Tone {
                frequency_hz,
                level,
            } => Self::Tone {
                frequency_hz,
                level,
            },
        };
        audio.validate().map_err(serde::de::Error::custom)?;
        Ok(audio)
    }
}

impl ReferenceAudibility {
    fn validate(self) -> Result<(), DocumentError> {
        if matches!(
            self,
            Self::Tail {
                maximum: FrameDuration::ZERO,
                ..
            }
        ) {
            return Err(invalid("frozen Tail maximum must be positive"));
        }
        if let Self::Custom {
            fade_in_ms,
            fade_out_ms,
            ..
        } = self
            && (fade_in_ms > crate::MAX_CUSTOM_FADE_MS || fade_out_ms > crate::MAX_CUSTOM_FADE_MS)
        {
            return Err(invalid("frozen custom sound fades for at most 10 s"));
        }
        Ok(())
    }

    fn validate_duration(self, duration: FrameDuration) -> Result<(), DocumentError> {
        self.validate()?;
        if let Self::Tail { maximum, .. } = self
            && maximum > duration
        {
            return Err(invalid(
                "frozen Tail maximum exceeds its Hold or gap duration",
            ));
        }
        Ok(())
    }
}

impl From<&HoldAudio> for ReferenceAudibility {
    fn from(audio: &HoldAudio) -> Self {
        match audio {
            HoldAudio::Silence => Self::Silence,
            HoldAudio::RoomTone { .. } => Self::RoomTone,
            HoldAudio::Tail {
                maximum,
                effect,
                input,
            } => Self::Tail {
                maximum: *maximum,
                effect: *effect,
                input: *input,
            },
            HoldAudio::Reverse { .. } => Self::Reverse,
            HoldAudio::Rewind { bounce } => Self::Rewind { bounce: *bounce },
            HoldAudio::Custom {
                looped,
                fade_in_ms,
                fade_out_ms,
                ..
            } => Self::Custom {
                looped: *looped,
                fade_in_ms: *fade_in_ms,
                fade_out_ms: *fade_out_ms,
            },
            HoldAudio::Tone {
                frequency_hz,
                level,
            } => Self::Tone {
                frequency_hz: *frequency_hz,
                level: *level,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum FrozenAudioKind {
    Source {
        #[serde(deserialize_with = "required_placement")]
        placement: Option<ExactFrameRange>,
    },
    Hold {
        audio: ReferenceAudibility,
    },
    Sequence {
        children: Vec<NodeId>,
    },
    Repeat {
        child: NodeId,
        iterations: IterationOrder,
        gap_duration: FrameDuration,
        gap_audio: ReferenceAudibility,
    },
    Retime {
        child: NodeId,
        mapping: FrameRange,
        pitch: PitchPolicy,
        purpose: RetimePurpose,
    },
}

fn required_placement<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<ExactFrameRange>, D::Error> {
    Option::deserialize(deserializer)
}

impl FrozenAudioKind {
    fn children(&self) -> &[NodeId] {
        match self {
            Self::Sequence { children } => children,
            Self::Repeat { child, .. } | Self::Retime { child, .. } => std::slice::from_ref(child),
            Self::Source { .. } | Self::Hold { .. } => &[],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenAudioNode {
    pub duration: FrameDuration,
    #[serde(default, skip_serializing_if = "AudioEdgePolicies::is_automatic")]
    pub edges: AudioEdgePolicies,
    #[serde(default, skip_serializing_if = "crate::AudioEditorialEdges::is_empty")]
    pub editorial_edges: crate::AudioEditorialEdges,
    pub kind: FrozenAudioKind,
}

#[derive(Debug, Clone, Default)]
struct FrozenIndex {
    /// Looked up per projection step and never iterated, so hashed.
    parents: crate::id_hash::IdMap<NodeId, (NodeId, ExactRatio)>,
    repeats: BTreeMap<NodeId, crate::ExactRepeatLayout>,
    /// Empty for ordinary layouts, whose stored recipe counts are exact.
    extents: BTreeMap<NodeId, ExactRatio>,
    root_duration: Option<FrameDuration>,
}

/// Admitted through capture or bounded JSON only. Cached structural indexes
/// make projection independent of layout size and never expand Repeat plays.
///
/// Layouts are immutable once admitted, so clones share one allocation.
/// Copying a document, binding state or patch therefore never copies a large
/// retained table, and comparing two copies of the same table is immediate.
#[derive(Debug, Clone)]
pub struct FrozenAudioLayout {
    inner: Arc<FrozenAudioLayoutData>,
}

/// The shared contents of a [`FrozenAudioLayout`]; no fields are public.
#[doc(hidden)]
#[derive(Debug, Serialize)]
pub struct FrozenAudioLayoutData {
    root: NodeId,
    rate: FrameRate,
    nodes: BTreeMap<NodeId, FrozenAudioNode>,
    overrides: BTreeMap<NodeId, PlayOverrides>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    gap_overrides: BTreeMap<NodeId, PlayOverrides>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    audio_lineage: BTreeMap<NodeId, AudioLineageId>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    derived_clocks: BTreeMap<NodeId, FrozenRetimeClock>,
    #[serde(skip)]
    index: FrozenIndex,
    /// The exact compact JSON length, remembered once counted. The contents
    /// are immutable, so the length cannot change.
    #[serde(skip)]
    wire_bytes: std::sync::OnceLock<usize>,
}

impl std::ops::Deref for FrozenAudioLayout {
    type Target = FrozenAudioLayoutData;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl Serialize for FrozenAudioLayout {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.inner.serialize(serializer)
    }
}

impl PartialEq for FrozenAudioLayout {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
            || (self.root == other.root
                && self.rate == other.rate
                && self.nodes == other.nodes
                && self.overrides == other.overrides
                && self.gap_overrides == other.gap_overrides
                && self.audio_lineage == other.audio_lineage
                && self.derived_clocks == other.derived_clocks)
    }
}
impl Eq for FrozenAudioLayout {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LayoutWire {
    root: NodeId,
    rate: FrameRate,
    #[serde(deserialize_with = "unique_map")]
    nodes: BTreeMap<NodeId, FrozenAudioNode>,
    #[serde(deserialize_with = "unique_map")]
    overrides: BTreeMap<NodeId, PlayOverrides>,
    #[serde(default, deserialize_with = "unique_map")]
    gap_overrides: BTreeMap<NodeId, PlayOverrides>,
    #[serde(default, deserialize_with = "unique_map")]
    audio_lineage: BTreeMap<NodeId, AudioLineageId>,
    #[serde(default, deserialize_with = "unique_map")]
    derived_clocks: BTreeMap<NodeId, FrozenRetimeClock>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FrozenAudioProjection {
    pub origin: ExactRatio,
    pub frames_per_local_frame: ExactRatio,
    pub point: ExactRatio,
    pub local_duration: FrameDuration,
    /// Effective allocation, independent of the integral processing recipe.
    pub local_extent: ExactRatio,
    pub instance: InstancePath,
    pub gap_after: Option<IterationId>,
    /// Charged nodes and compact identity segments, not rendered play count.
    pub work: usize,
}

/// A bounded correspondence over a sound's proven clock dependencies.
/// Complete-subtree queries pair every child; owner-aware queries may prove only
/// an independent output owner and its placement ancestry. No omitted child is
/// a proven alias. The proof owns bounded scoped indexes, not either layout.
#[derive(Debug, Clone)]
pub struct SoundClockCorrespondence {
    inner: Arc<SoundClockCorrespondenceInner>,
}

enum SoundClockProofOwners<'a> {
    /// A current delivery proof, retained only up to the nearest processor
    /// that both contexts share and that consumes all of their pieces.
    Delivery,
    None,
    Pair(&'a NodeId, &'a NodeId),
    RootSplit,
}

#[derive(Debug)]
struct SoundClockCorrespondenceInner {
    live_to_historical: BTreeMap<NodeId, NodeId>,
    split_aliases: BTreeMap<NodeId, NodeId>,
    introduced: BTreeMap<NodeId, IterationOrder>,
    historical_placement_repeats: BTreeSet<NodeId>,
    historical_path: ScopedPathIndex,
    live_path: ScopedPathIndex,
    work: usize,
}

#[derive(Debug, Clone)]
struct ScopedPathIndex {
    scope: NodeId,
    parents: BTreeMap<NodeId, NodeId>,
    repeats: BTreeMap<NodeId, ScopedRepeat>,
}

#[derive(Debug, Clone)]
struct ScopedRepeat {
    iterations: IterationOrder,
    child: NodeId,
    overrides: PlayOverrides,
    gap_overrides: PlayOverrides,
}

impl SoundClockCorrespondence {
    /// Work charged while proving the paired clock dependencies.
    pub fn work(&self) -> usize {
        self.inner.work
    }

    /// Historical node corresponding to a live node in the paired scope.
    pub fn historical_node(&self, live: &NodeId) -> Option<&NodeId> {
        self.inner.live_to_historical.get(live)
    }

    /// Exact leaf aliases established by an explicit complete Split forest.
    /// Delivery Partitions themselves never acquire a historical alias.
    pub fn split_alias(&self, live: &NodeId) -> Option<&NodeId> {
        self.inner.split_aliases.get(live)
    }

    /// All proven pairs, ordered by the live NodeId.
    pub fn node_pairs(&self) -> impl Iterator<Item = (&NodeId, &NodeId)> {
        self.inner.live_to_historical.iter()
    }

    /// Validate and translate a live occurrence path into its historical alias.
    /// Repeat identities are stable across the paired processing scopes.
    pub fn remap_instance(&self, live: &InstancePath) -> Result<InstancePath, DocumentError> {
        self.remap_instance_with_work(live, MAX_DOCUMENT_NODES)
            .map(|(historical, _)| historical)
    }

    /// Remap an instance and return the bounded path-validation work charged.
    pub fn remap_instance_with_work(
        &self,
        live: &InstancePath,
        maximum_work: usize,
    ) -> Result<(InstancePath, usize), DocumentError> {
        let (instance, work) = self.try_remap_instance_with_work(live, maximum_work)?;
        Ok((
            instance.ok_or_else(|| invalid("sound occurrence was not present in this clock"))?,
            work,
        ))
    }

    /// A valid live play born after this placement has no historical occurrence.
    /// Only explicitly mapped external Repeat ancestors may establish absence.
    pub fn try_remap_instance_with_work(
        &self,
        live: &InstancePath,
        maximum_work: usize,
    ) -> Result<(Option<InstancePath>, usize), DocumentError> {
        if maximum_work == 0 || maximum_work > MAX_DOCUMENT_NODES {
            return Err(limit("invalid sound clock path budget"));
        }
        let mut live_work = validate_scoped_instance(&self.inner.live_path, live, maximum_work)?;
        let mut introduced_present = true;
        for step in &live.repeats {
            if let Some(plays) = self.inner.introduced.get(&step.node) {
                spend(&mut live_work, plays.segment_count(), maximum_work)?;
                introduced_present &= plays.position(&step.iteration).is_some();
            }
        }
        let historical = InstancePath {
            node: self
                .historical_node(&live.node)
                .cloned()
                .ok_or_else(|| invalid("sound clock occurrence is outside its live scope"))?,
            repeats: live
                .repeats
                .iter()
                .filter(|step| !self.inner.introduced.contains_key(&step.node))
                .map(|step| {
                    Ok(crate::RepeatInstance {
                        node: self.historical_node(&step.node).cloned().ok_or_else(|| {
                            invalid("sound clock Repeat is outside its live scope")
                        })?,
                        iteration: step.iteration.clone(),
                    })
                })
                .collect::<Result<Vec<_>, DocumentError>>()?,
        };
        let remaining = maximum_work
            .checked_sub(live_work)
            .filter(|remaining| *remaining > 0)
            .ok_or_else(|| limit("sound clock occurrence work exhausted"))?;
        let (present, historical_work) = validate_scoped_instance_presence(
            &self.inner.historical_path,
            &historical,
            &self.inner.historical_placement_repeats,
            remaining,
        )?;
        let work = live_work
            .checked_add(historical_work)
            .ok_or_else(|| limit("sound clock occurrence work overflow"))?;
        Ok(((present && introduced_present).then_some(historical), work))
    }
}

pub(crate) struct ScopedProjectionRequest<'a> {
    pub(crate) root: &'a NodeId,
    pub(crate) instance: &'a InstancePath,
    pub(crate) local: ExactRatio,
    pub(crate) gap_after: Option<&'a IterationId>,
    pub(crate) maximum_work: usize,
    pub(crate) mode: ProjectionMode,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectionMode {
    Affine,
    MeaningfulSupport,
    VisibleAllocation,
}

impl FrozenAudioLayout {
    /// Closed grammar for layouts embedded in older documents and contexts.
    /// Reject presence of the new field even when both flags are false.
    pub(crate) fn validate_pre_editorial_json(json: &str) -> Result<(), DocumentError> {
        preflight::check_pre_editorial(json)
    }

    pub(crate) fn has_editorial_edges(&self) -> bool {
        self.nodes
            .values()
            .any(|node| !node.editorial_edges.is_empty())
    }

    /// Rebuild all indexes after a typed historical identity rename.
    pub(crate) fn from_renamed_parts(
        root: NodeId,
        rate: FrameRate,
        nodes: BTreeMap<NodeId, FrozenAudioNode>,
        overrides: BTreeMap<NodeId, PlayOverrides>,
        gap_overrides: BTreeMap<NodeId, PlayOverrides>,
        audio_lineage: BTreeMap<NodeId, AudioLineageId>,
        derived_clocks: BTreeMap<NodeId, FrozenRetimeClock>,
    ) -> Result<Self, DocumentError> {
        Self::admit(LayoutWire {
            root,
            rate,
            nodes,
            overrides,
            gap_overrides,
            audio_lineage,
            derived_clocks,
        })
    }
    pub fn capture(document: &ProjectDocument) -> Result<Self, DocumentError> {
        Self::capture_inner(document, true)
    }

    /// Binding validation cannot recursively validate the binding state while
    /// constructing its live comparison clock. Structural validation still runs.
    pub(crate) fn capture_structural(document: &ProjectDocument) -> Result<Self, DocumentError> {
        Self::capture_inner(document, false)
    }

    fn capture_inner(
        document: &ProjectDocument,
        validate_bindings: bool,
    ) -> Result<Self, DocumentError> {
        Self::precharge(document)?;
        let durations = if validate_bindings {
            document.recipe_durations_shared()?
        } else {
            let recipes = document.recipe_structural_durations()?;
            if !document.derived_clocks().is_empty() {
                document.effective_durations(&recipes)?;
            }
            Arc::new(recipes)
        };
        let rate = document.presentation_basis().frame_rate;
        let mut nodes = BTreeMap::new();
        for (id, node) in document.nodes() {
            nodes.insert(id.clone(), frozen_node(node, durations[id], rate, true)?);
        }
        Self::admit(LayoutWire {
            root: document.root().clone(),
            rate,
            nodes,
            overrides: document.overrides().clone(),
            gap_overrides: document.gap_overrides().clone(),
            audio_lineage: document.audio_lineage().clone(),
            derived_clocks: document
                .derived_clocks()
                .iter()
                .map(|(id, clock)| {
                    (
                        id.clone(),
                        FrozenRetimeClock {
                            extent: clock.extent,
                            child_start: clock.child_start,
                            child_end: clock.child_end,
                        },
                    )
                })
                .collect(),
        })
    }

    /// Precharge before cloning nodes, overrides or iteration vectors and
    /// before durations() builds any derived Repeat layouts. Returns the
    /// document's total compact run count.
    fn precharge(document: &ProjectDocument) -> Result<usize, DocumentError> {
        let mut edges = 0usize;
        let mut runs = 0usize;
        if document.audio_lineage().len() > MAX_DOCUMENT_NODES {
            return Err(limit("frozen audio lineage exceeds node limit"));
        }
        // Override roots are edges of their Repeat; counting them per owner
        // map rather than per node gives the same running totals at the end
        // of each node, without a map lookup per node.
        fn branches<'a>((owner, entries): (&'a NodeId, &PlayOverrides)) -> (&'a NodeId, usize) {
            (owner, entries.iter().count())
        }
        let mut overrides = document.overrides().iter().map(branches).peekable();
        let mut gap_overrides = document.gap_overrides().iter().map(branches).peekable();
        for (id, node) in document.nodes() {
            let mut count = node.kind.children().len();
            for branches in [&mut overrides, &mut gap_overrides] {
                // Both maps iterate in the same key order as the nodes; an
                // owner that is not a node never matches (validation rejects
                // it elsewhere, and `children` ignored it too).
                while branches.peek().is_some_and(|(owner, _)| *owner < id) {
                    branches.next();
                }
                if let Some((_, entries)) = branches.next_if(|(owner, _)| *owner == id) {
                    count += entries;
                }
            }
            edges = edges
                .checked_add(count)
                .ok_or_else(|| limit("frozen structural edge count overflow"))?;
            if let NodeKind::Repeat { iterations, .. } = &node.kind {
                runs = runs
                    .checked_add(iterations.segment_count())
                    .ok_or_else(|| limit("frozen compact run count overflow"))?;
            }
            if edges > MAX_DOCUMENT_NODES || runs > MAX_FROZEN_AUDIO_RUNS {
                return Err(limit("document exceeds frozen reference complexity limits"));
            }
        }
        Ok(runs)
    }

    /// Whether `id` names a node: the root or an indexed child. Admission
    /// proved every node reachable, so this equals `nodes.contains_key`
    /// without an ordered lookup.
    fn has_node(&self, id: &NodeId) -> bool {
        let found = *id == self.root || self.index.parents.contains_key(id);
        debug_assert_eq!(found, self.nodes.contains_key(id));
        found
    }

    fn admit(wire: LayoutWire) -> Result<Self, DocumentError> {
        Self::admit_sized(wire, true)
    }

    fn admit_sized(wire: LayoutWire, check_size: bool) -> Result<Self, DocumentError> {
        let layout = Self {
            inner: Arc::new(FrozenAudioLayoutData {
                root: wire.root,
                rate: wire.rate,
                nodes: wire.nodes,
                overrides: wire.overrides,
                gap_overrides: wire.gap_overrides,
                audio_lineage: wire.audio_lineage,
                derived_clocks: wire.derived_clocks,
                index: FrozenIndex::default(),
                wire_bytes: std::sync::OnceLock::new(),
            }),
        };
        let index = layout.build_index()?;
        let mut data = Arc::into_inner(layout.inner).expect("a new layout has one owner");
        data.index = index;
        let layout = Self {
            inner: Arc::new(data),
        };
        // Capture and JSON admission share the serialized size ceiling.
        // Counting the bytes is the same check without retaining the text.
        if check_size {
            if crate::command_work::local() {
                layout.wire_bytes()?;
            } else {
                layout.to_json()?;
            }
        }
        Ok(layout)
    }

    /// The exact compact JSON length of this layout, which [`Self::to_json`]
    /// would produce, counted once and remembered.
    pub(crate) fn wire_bytes(&self) -> Result<usize, DocumentError> {
        if let Some(bytes) = self.inner.wire_bytes.get() {
            return Ok(*bytes);
        }
        struct Count {
            bytes: usize,
            exceeded: bool,
        }
        impl Write for Count {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                if bytes.len() > MAX_DOCUMENT_JSON_BYTES - self.bytes {
                    self.exceeded = true;
                    return Err(std::io::Error::other(
                        "frozen audio JSON exceeds byte limit",
                    ));
                }
                self.bytes += bytes.len();
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut count = Count {
            bytes: 0,
            exceeded: false,
        };
        serde_json::to_writer(&mut count, self).map_err(|error| {
            if count.exceeded {
                limit("frozen audio JSON exceeds byte limit")
            } else {
                DocumentError::json(error)
            }
        })?;
        Ok(*self.inner.wire_bytes.get_or_init(|| count.bytes))
    }

    pub fn from_json(json: &str) -> Result<Self, DocumentError> {
        if json.len() > MAX_DOCUMENT_JSON_BYTES {
            return Err(limit("frozen audio JSON exceeds byte limit"));
        }
        preflight::check(json)?;
        // The input already met the byte bound. Its canonical form can differ
        // only by a defaulted Tail effect, and every containing document is
        // bounded again whenever it is serialized for storage.
        Self::admit_sized(
            serde_json::from_str(json).map_err(DocumentError::json)?,
            false,
        )
    }

    pub(crate) fn preflight_binding_counts(
        json: &str,
    ) -> Result<(usize, usize, usize), DocumentError> {
        preflight::complexity(json)
    }

    pub fn to_json(&self) -> Result<String, DocumentError> {
        let mut output = BoundedJson {
            bytes: Vec::new(),
            exceeded: false,
        };
        serde_json::to_writer(&mut output, self).map_err(|error| {
            if output.exceeded {
                limit("frozen audio JSON exceeds byte limit")
            } else {
                DocumentError::json(error)
            }
        })?;
        String::from_utf8(output.bytes)
            .map_err(|error| DocumentError::new(DocumentErrorCode::InvalidJson, error.to_string()))
    }

    pub fn validate(&self) -> Result<(), DocumentError> {
        self.build_index().map(|_| ())
    }
    pub fn root(&self) -> &NodeId {
        &self.root
    }
    pub fn rate(&self) -> FrameRate {
        self.rate
    }
    pub fn nodes(&self) -> &BTreeMap<NodeId, FrozenAudioNode> {
        &self.nodes
    }
    pub fn overrides(&self) -> &BTreeMap<NodeId, PlayOverrides> {
        &self.overrides
    }
    pub fn gap_overrides(&self) -> &BTreeMap<NodeId, PlayOverrides> {
        &self.gap_overrides
    }
    /// Authored copy provenance keyed by owned frozen aliases. Token origins
    /// are historical names, never live references or admission to media/PCM.
    pub fn audio_lineage(&self) -> &BTreeMap<NodeId, AudioLineageId> {
        &self.audio_lineage
    }
    pub fn duration(&self) -> FrameDuration {
        self.index
            .root_duration
            .unwrap_or(self.nodes[&self.root].duration)
    }

    /// Prove that two complete processing subtrees correspond, allowing fresh
    /// node IDs while preserving exact clocks, Repeat identities, and controls.
    /// The scopes themselves cannot be project roots; only ordinary Sequence
    /// nodes, Repeats and transparent Partitions may surround either scope.
    pub fn sound_clock_correspondence(
        &self,
        current: &Self,
        historical_scope: &NodeId,
        live_scope: &NodeId,
        maximum_work: usize,
    ) -> Result<SoundClockCorrespondence, DocumentError> {
        self.sound_clock_correspondence_with_repeats(
            current,
            historical_scope,
            live_scope,
            &crate::SoundClockRepeatMap::default(),
            maximum_work,
        )
    }

    /// Pair unchanged processing scopes with an explicit map of their external
    /// Repeat placements. No Retime processor may be omitted from the scope.
    pub fn sound_clock_correspondence_with_repeats(
        &self,
        current: &Self,
        historical_scope: &NodeId,
        live_scope: &NodeId,
        repeats: &crate::SoundClockRepeatMap,
        maximum_work: usize,
    ) -> Result<SoundClockCorrespondence, DocumentError> {
        self.sound_clock_correspondence_inner(
            current,
            (historical_scope, live_scope),
            repeats,
            SoundClockProofOwners::None,
            (&BTreeSet::new(), &[], &[]),
            maximum_work,
        )
    }

    /// Whether this explicit owner pair permits the narrower output-only proof.
    /// Correspondence caches must include that owner pair in their key; full
    /// subtree proofs may share an exact scope-and-relation key across owners.
    pub fn sound_uses_output_owner_proof(
        &self,
        current: &Self,
        scopes: (&NodeId, &NodeId),
        owners: (&NodeId, &NodeId),
    ) -> bool {
        owners == scopes
            && matches!(
                self.nodes.get(scopes.0).map(|node| &node.kind),
                Some(FrozenAudioKind::Retime {
                    purpose: RetimePurpose::Edit,
                    ..
                })
            )
            && matches!(
                current.nodes.get(scopes.1).map(|node| &node.kind),
                Some(FrozenAudioKind::Retime {
                    purpose: RetimePurpose::Edit,
                    ..
                })
            )
    }

    /// Prove one independent event's clock, with an explicit historical/live
    /// owner pair from that event's journal or the command's owner mapping.
    /// Equal-looking nodes never discover an alias. A fixed Edit Retime's own
    /// sound enters its output, so its changed child is not a dependency.
    /// Descendant owners and scopes containing another processor retain the
    /// complete-subtree contract. Current source admission and policy still
    /// apply independently of this address/clock proof.
    pub fn sound_owner_clock_correspondence_with_repeats(
        &self,
        current: &Self,
        historical_scope: &NodeId,
        live_scope: &NodeId,
        owners: (&NodeId, &NodeId),
        repeats: &crate::SoundClockRepeatMap,
        maximum_work: usize,
    ) -> Result<SoundClockCorrespondence, DocumentError> {
        self.sound_owner_clock_correspondence_with_groups(
            current,
            (historical_scope, live_scope),
            owners,
            repeats,
            &BTreeSet::new(),
            maximum_work,
        )
    }

    /// Pair one independent sound owner while passing through only explicitly
    /// introduced ordinary Sequence wrappers inside its live scope. A listed
    /// scope itself may only be a unary chain to the recorded Source/Hold owner
    /// when the historical scope is that historical owner. Every old node still
    /// has an ordered pair; a new wrapper has no historical alias.
    /// This is clock correspondence only, not source or processing authority.
    pub fn sound_owner_clock_correspondence_with_groups(
        &self,
        current: &Self,
        scopes: (&NodeId, &NodeId),
        owners: (&NodeId, &NodeId),
        repeats: &crate::SoundClockRepeatMap,
        introduced_groups: &BTreeSet<NodeId>,
        maximum_work: usize,
    ) -> Result<SoundClockCorrespondence, DocumentError> {
        self.sound_owner_clock_correspondence_with_relations(
            current,
            scopes,
            owners,
            repeats,
            (introduced_groups, &[], &[]),
            maximum_work,
        )
    }

    /// Opt-in correspondence through explicitly certified complete Split
    /// forests, in addition to introduced ordinary groups. Every old alias
    /// stays paired; only certified full leaf copies may share an old leaf.
    /// The relation pair contains introduced groups followed by Split forests.
    pub fn sound_owner_clock_correspondence_with_relations(
        &self,
        current: &Self,
        scopes: (&NodeId, &NodeId),
        owners: (&NodeId, &NodeId),
        repeats: &crate::SoundClockRepeatMap,
        introduced: (
            &BTreeSet<NodeId>,
            &[crate::SoundClockSplit],
            &[crate::SoundClockIsolation],
        ),
        maximum_work: usize,
    ) -> Result<SoundClockCorrespondence, DocumentError> {
        self.sound_clock_correspondence_inner(
            current,
            scopes,
            repeats,
            SoundClockProofOwners::Pair(owners.0, owners.1),
            introduced,
            maximum_work,
        )
    }

    /// A duration-preserving child Split leaves a root attachment at the same
    /// output stage. Prove its complete input without creating a root journal.
    /// Root clock references remain unsupported by the public journal API.
    pub(crate) fn sound_split_root_correspondence(
        &self,
        current: &Self,
        groups: &BTreeSet<NodeId>,
        splits: &[crate::SoundClockSplit],
        maximum_work: usize,
    ) -> Result<SoundClockCorrespondence, DocumentError> {
        self.sound_clock_correspondence_inner(
            current,
            (&self.root, &current.root),
            &crate::SoundClockRepeatMap::default(),
            SoundClockProofOwners::RootSplit,
            (groups, splits, &[]),
            maximum_work,
        )
    }

    /// [`Self::sound_clock_correspondence_attempt`] for contexts that may sit
    /// inside a processor shared by every delivered piece.
    pub fn sound_clock_delivery_correspondence_attempt(
        &self,
        current: &Self,
        scopes: (&NodeId, &NodeId),
        repeats: &crate::SoundClockRepeatMap,
        maximum_work: usize,
    ) -> (Result<SoundClockCorrespondence, DocumentError>, usize) {
        self.sound_clock_correspondence_attempt_inner(
            current,
            scopes,
            repeats,
            SoundClockProofOwners::Delivery,
            (&BTreeSet::new(), &[], &[]),
            maximum_work,
        )
    }

    /// Attempt a complete correspondence, reporting consumed work even when
    /// a late structural mismatch rejects it. Candidate-search callers must
    /// charge this count before trying another pair.
    pub fn sound_clock_correspondence_attempt(
        &self,
        current: &Self,
        scopes: (&NodeId, &NodeId),
        repeats: &crate::SoundClockRepeatMap,
        maximum_work: usize,
    ) -> (Result<SoundClockCorrespondence, DocumentError>, usize) {
        self.sound_clock_correspondence_attempt_inner(
            current,
            scopes,
            repeats,
            SoundClockProofOwners::None,
            (&BTreeSet::new(), &[], &[]),
            maximum_work,
        )
    }

    /// The complete Repeat ancestry between two retained contexts. A journal's
    /// `recorded` map stops at its own scope, so a context below that scope may
    /// have further Repeats. Only one surviving under the same identity, or a
    /// copy the journal's own isolation records, pairs without a recorded step;
    /// anything else stays unproved.
    pub fn sound_clock_context_repeats(
        &self,
        current: &Self,
        scopes: (&NodeId, &NodeId),
        recorded: &crate::SoundClockRepeatMap,
        isolations: &[crate::SoundClockIsolation],
        maximum_work: usize,
    ) -> (Result<crate::SoundClockRepeatMap, DocumentError>, usize) {
        let mut work = 0usize;
        let result = (|| {
            let (historical_scope, live_scope) = scopes;
            let live = placement_path(
                current,
                live_scope,
                true,
                &mut BTreeMap::new(),
                &mut BTreeMap::new(),
                maximum_work,
                &mut work,
            )?;
            let historical = placement_path(
                self,
                historical_scope,
                true,
                &mut BTreeMap::new(),
                &mut BTreeMap::new(),
                maximum_work,
                &mut work,
            )?;
            if enclosing_processor(current, live_scope)
                != enclosing_processor(self, historical_scope)
            {
                return Err(invalid("sound clock contexts have different processors"));
            }
            let mut remaining = historical.iter();
            let mut steps = Vec::with_capacity(live.len());
            for repeat in &live {
                spend(&mut work, 1 + recorded.steps().len(), maximum_work)?;
                let step = recorded
                    .steps()
                    .iter()
                    .find(|step| step.live_repeat() == repeat)
                    .cloned();
                let shared = !matches!(step, Some(crate::SoundClockRepeatStep::Introduced { .. }));
                let paired = if shared { remaining.next() } else { None };
                steps.push(match (step, paired) {
                    (Some(step), None) if !shared => step,
                    (
                        Some(crate::SoundClockRepeatStep::Shared {
                            historical_repeat, ..
                        }),
                        Some(historical),
                    ) if &historical_repeat == historical => crate::SoundClockRepeatStep::Shared {
                        live_repeat: repeat.clone(),
                        historical_repeat,
                    },
                    (None, Some(historical))
                        if historical == repeat
                            || isolations.iter().any(|isolation| {
                                &isolation.live_repeat == repeat
                                    && &isolation.historical_repeat == historical
                            }) =>
                    {
                        crate::SoundClockRepeatStep::Shared {
                            live_repeat: repeat.clone(),
                            historical_repeat: historical.clone(),
                        }
                    }
                    _ => {
                        return Err(invalid(
                            "sound clock Repeat map does not describe its complete ancestry",
                        ));
                    }
                });
            }
            if remaining.next().is_some() {
                return Err(invalid(
                    "sound clock Repeat map does not describe its complete ancestry",
                ));
            }
            crate::SoundClockRepeatMap::new(steps)
        })();
        (result, work)
    }

    fn sound_clock_correspondence_inner(
        &self,
        current: &Self,
        scopes: (&NodeId, &NodeId),
        repeats: &crate::SoundClockRepeatMap,
        owner_proof: SoundClockProofOwners<'_>,
        introduced: (
            &BTreeSet<NodeId>,
            &[crate::SoundClockSplit],
            &[crate::SoundClockIsolation],
        ),
        maximum_work: usize,
    ) -> Result<SoundClockCorrespondence, DocumentError> {
        self.sound_clock_correspondence_attempt_inner(
            current,
            scopes,
            repeats,
            owner_proof,
            introduced,
            maximum_work,
        )
        .0
    }

    fn sound_clock_correspondence_attempt_inner(
        &self,
        current: &Self,
        scopes: (&NodeId, &NodeId),
        repeats: &crate::SoundClockRepeatMap,
        owner_proof: SoundClockProofOwners<'_>,
        introduced: (
            &BTreeSet<NodeId>,
            &[crate::SoundClockSplit],
            &[crate::SoundClockIsolation],
        ),
        maximum_work: usize,
    ) -> (Result<SoundClockCorrespondence, DocumentError>, usize) {
        let mut work = 0usize;
        let result = (|| {
            let (historical_scope, live_scope) = scopes;
            let (introduced_groups, introduced_splits, introduced_isolations) = introduced;
            let root_split = matches!(owner_proof, SoundClockProofOwners::RootSplit);
            let owners = match owner_proof {
                SoundClockProofOwners::Pair(historical, live) => Some((historical, live)),
                _ => None,
            };
            if maximum_work == 0 || maximum_work > MAX_DOCUMENT_NODES {
                return Err(limit("invalid sound clock comparison budget"));
            }
            if (historical_scope == &self.root || live_scope == &current.root)
                && !(root_split
                    && historical_scope == &self.root
                    && live_scope == &current.root
                    && !introduced_splits.is_empty())
            {
                return Err(invalid("root-owned sound clocks are not supported"));
            }
            if self.rate != current.rate {
                return Err(invalid("sound clock project rate changed"));
            }
            if !self.nodes.contains_key(historical_scope) || !current.nodes.contains_key(live_scope)
            {
                return Err(invalid("sound clock scope is missing"));
            }

            let mut split_state = split_correspondence::SplitProofState::new(
                introduced_splits,
                &mut work,
                maximum_work,
            )?;
            let mut isolation_state = isolation_correspondence::IsolationProofState::new(
                introduced_isolations,
                &mut work,
                maximum_work,
            )?;
            // Charge the complete relation before looking up its entries or
            // allocating the visited set. Unused and exterior entries are rejected
            // after the subtree walk, rather than searching unrelated branches.
            spend(&mut work, introduced_groups.len(), maximum_work)?;
            for group in introduced_groups {
                if group == &current.root
                    || self.nodes.contains_key(group)
                    || owners.is_some_and(|(_, live_owner)| group == live_owner)
                    || current.derived_clocks.contains_key(group)
                    || !matches!(
                        current.nodes.get(group).map(|node| &node.kind),
                        Some(FrozenAudioKind::Sequence { .. })
                    )
                {
                    return Err(invalid("invalid introduced sound clock group"));
                }
            }
            let mut visited_groups = BTreeSet::new();
            let mut live_to_historical = BTreeMap::new();
            let mut historical_seen = BTreeSet::new();
            let mut live_parents = BTreeMap::new();
            let mut historical_parents = BTreeMap::new();
            let mut live_repeats = BTreeMap::new();
            let mut historical_repeats = BTreeMap::new();
            let inside_processor = matches!(owner_proof, SoundClockProofOwners::Delivery);
            let live_ancestors = placement_path(
                current,
                live_scope,
                inside_processor,
                &mut live_parents,
                &mut live_repeats,
                maximum_work,
                &mut work,
            )?;
            let historical_ancestors = placement_path(
                self,
                historical_scope,
                inside_processor,
                &mut historical_parents,
                &mut historical_repeats,
                maximum_work,
                &mut work,
            )?;
            spend(&mut work, repeats.steps().len(), maximum_work)?;
            let mapped_live: Vec<_> = repeats
                .steps()
                .iter()
                .map(|step| step.live_repeat().clone())
                .collect();
            let mapped_historical: Vec<_> = repeats
                .steps()
                .iter()
                .filter_map(|step| match step {
                    crate::SoundClockRepeatStep::Shared {
                        historical_repeat, ..
                    } => Some(historical_repeat.clone()),
                    crate::SoundClockRepeatStep::Introduced { .. } => None,
                })
                .collect();
            if mapped_live != live_ancestors || mapped_historical != historical_ancestors {
                return Err(invalid(
                    "sound clock Repeat map does not describe its complete ancestry",
                ));
            }
            let mut introduced = BTreeMap::new();
            for step in repeats.steps() {
                match step {
                    crate::SoundClockRepeatStep::Shared {
                        live_repeat,
                        historical_repeat,
                    } => {
                        live_to_historical.insert(live_repeat.clone(), historical_repeat.clone());
                        historical_seen.insert(historical_repeat.clone());
                    }
                    crate::SoundClockRepeatStep::Introduced { live_repeat, plays } => {
                        spend(&mut work, plays.segment_count(), maximum_work)?;
                        introduced.insert(live_repeat.clone(), plays.clone());
                    }
                }
            }
            let mut pending = if introduced_groups.contains(live_scope) {
                let (historical_owner, live_owner) = owners.ok_or_else(|| {
                    invalid("introduced sound clock scope requires an owner pair")
                })?;
                if historical_scope != historical_owner
                    || !matches!(
                        self.nodes.get(historical_owner).map(|node| &node.kind),
                        Some(FrozenAudioKind::Source { .. } | FrozenAudioKind::Hold { .. })
                    )
                    || !matches!(
                        current.nodes.get(live_owner).map(|node| &node.kind),
                        Some(FrozenAudioKind::Source { .. } | FrozenAudioKind::Hold { .. })
                    )
                    || self.derived_clocks.contains_key(historical_owner)
                    || current.derived_clocks.contains_key(live_owner)
                    || self.node_extent(historical_owner)? != current.node_extent(live_scope)?
                {
                    return Err(invalid(
                        "introduced sound clock scope is not one complete owner",
                    ));
                }
                // Retain the narrow unary-root contract, extended only by an exact
                // certified full Split forest at the end of that recorded chain.
                let mut group = live_scope;
                loop {
                    spend(&mut work, 1, maximum_work)?;
                    if !introduced_groups.contains(group) {
                        return Err(invalid(
                            "introduced sound clock scope has an unlisted wrapper",
                        ));
                    }
                    let FrozenAudioKind::Sequence { children } = &current.nodes[group].kind else {
                        return Err(invalid("introduced sound clock scope is not a Sequence"));
                    };
                    if children.len() == 1 {
                        spend(&mut work, 1, maximum_work)?;
                        if !current.index.parents.get(&children[0]).is_some_and(
                            |(parent, offset)| parent == group && *offset == ExactRatio::ZERO,
                        ) {
                            return Err(invalid(
                                "introduced sound clock scope has an invalid child edge",
                            ));
                        }
                    }
                    if children.len() == 1 && &children[0] == live_owner {
                        break;
                    }
                    if split_state.root_forest(historical_scope, children) {
                        break;
                    }
                    if children.len() != 1 || !introduced_groups.contains(&children[0]) {
                        return Err(invalid(
                            "introduced sound clock scope is not unary or a complete Split forest",
                        ));
                    }
                    group = &children[0];
                }
                let parent = &current.index.parents[live_scope].0;
                let depth = live_parents.len().saturating_sub(1);
                let pairs = split_correspondence::SequenceRelations {
                    historical: self,
                    current,
                    groups: introduced_groups,
                    visited_groups: &mut visited_groups,
                    live_parents: &mut live_parents,
                    historical_parents: &mut historical_parents,
                    splits: &mut split_state,
                    work: &mut work,
                    maximum: maximum_work,
                }
                .pair(
                    None,
                    std::slice::from_ref(historical_scope),
                    (parent, std::slice::from_ref(live_scope), depth),
                    Some(live_scope),
                )?;
                let mut pending = Vec::with_capacity(pairs.len());
                for pair in pairs {
                    if live_parents
                        .insert(pair.live.clone(), pair.live_parent)
                        .is_some()
                    {
                        return Err(invalid("introduced sound clock scope has a shared owner"));
                    }
                    pending.push((pair.historical, pair.live, pair.live_depth));
                }
                pending
            } else {
                vec![(
                    historical_scope.clone(),
                    live_scope.clone(),
                    live_parents.len(),
                )]
            };
            while let Some((historical_id, live_id, live_depth)) = pending.pop() {
                if live_depth > MAX_DOCUMENT_DEPTH {
                    return Err(limit("sound clock subtree depth exceeds limit"));
                }
                spend(&mut work, 1, maximum_work)?;
                if live_to_historical
                    .insert(live_id.clone(), historical_id.clone())
                    .is_some()
                    || !historical_seen.insert(historical_id.clone())
                        && split_state.aliases.get(&live_id) != Some(&historical_id)
                        && isolation_state.aliases.get(&live_id) != Some(&historical_id)
                {
                    return Err(invalid(
                        "sound clock subtree correspondence is not one-to-one",
                    ));
                }
                let historical_node = self
                    .nodes
                    .get(&historical_id)
                    .ok_or_else(|| invalid("historical sound clock node is missing"))?;
                let live_node = current
                    .nodes
                    .get(&live_id)
                    .ok_or_else(|| invalid("live sound clock node is missing"))?;
                let pair_work = sound_clock_pair_work(self, current, &historical_id, &live_id)?;
                // Charge both frozen sides before scanning Repeat runs/override keys
                // or allocating paired-child and compact path indexes.
                spend(&mut work, pair_work, maximum_work)?;
                if historical_node.duration != live_node.duration
                    || self.derived_clocks.get(&historical_id)
                        != current.derived_clocks.get(&live_id)
                    || !(same_sound_clock_kind(&historical_node.kind, &live_node.kind)
                        || (!introduced_groups.is_empty() || !introduced_splits.is_empty())
                            && matches!(
                                (&historical_node.kind, &live_node.kind),
                                (
                                    FrozenAudioKind::Sequence { .. },
                                    FrozenAudioKind::Sequence { .. }
                                )
                            ))
                {
                    return Err(invalid("sound clock processing subtree changed"));
                }
                if (split_state.aliases.get(&live_id) == Some(&historical_id)
                    || isolation_state.aliases.get(&live_id) == Some(&historical_id))
                    && (historical_node.edges != live_node.edges
                        || historical_node.editorial_edges != live_node.editorial_edges
                        || match (&historical_node.kind, &live_node.kind) {
                            (FrozenAudioKind::Source { .. }, FrozenAudioKind::Source { .. })
                            | (FrozenAudioKind::Hold { .. }, FrozenAudioKind::Hold { .. }) => {
                                historical_node.kind != live_node.kind
                            }
                            (
                                FrozenAudioKind::Repeat { gap_audio: old, .. },
                                FrozenAudioKind::Repeat {
                                    gap_audio: live, ..
                                },
                            ) => old != live,
                            _ => false,
                        })
                {
                    return Err(invalid("sound clock Split changed its complete context"));
                }

                // Only the explicitly paired event owner can terminate this
                // dependency walk. placement_path above already forbids omitting
                // an enclosing processor; the unchanged output duration, exact
                // clock and stage controls were checked before reaching here.
                let output_owner = historical_id == *historical_scope
                    && live_id == *live_scope
                    && introduced_splits.is_empty()
                    && introduced_isolations.is_empty()
                    && owners.is_some_and(|owners| {
                        self.sound_uses_output_owner_proof(
                            current,
                            (historical_scope, live_scope),
                            owners,
                        )
                    });
                let children = if output_owner {
                    Vec::new()
                } else if let (
                    FrozenAudioKind::Sequence {
                        children: historical,
                    },
                    FrozenAudioKind::Sequence { children: live },
                ) = (&historical_node.kind, &live_node.kind)
                {
                    split_correspondence::SequenceRelations {
                        historical: self,
                        current,
                        groups: introduced_groups,
                        visited_groups: &mut visited_groups,
                        live_parents: &mut live_parents,
                        historical_parents: &mut historical_parents,
                        splits: &mut split_state,
                        work: &mut work,
                        maximum: maximum_work,
                    }
                    .pair(
                        Some(&historical_id),
                        historical,
                        (&live_id, live, live_depth),
                        None,
                    )?
                } else {
                    let mut children = Vec::new();
                    for (historical, live) in isolation_state.children(
                        self,
                        current,
                        &historical_id,
                        &live_id,
                        &mut work,
                        maximum_work,
                    )? {
                        if introduced_groups.contains(&live) {
                            if !split_state.wrapped_forest(
                                &historical,
                                &live,
                                current,
                                introduced_groups,
                                &mut work,
                                maximum_work,
                            )? {
                                return Err(invalid("sound clock processing subtree changed"));
                            }
                            children.extend(
                                split_correspondence::SequenceRelations {
                                    historical: self,
                                    current,
                                    groups: introduced_groups,
                                    visited_groups: &mut visited_groups,
                                    live_parents: &mut live_parents,
                                    historical_parents: &mut historical_parents,
                                    splits: &mut split_state,
                                    work: &mut work,
                                    maximum: maximum_work,
                                }
                                .pair(
                                    Some(&historical_id),
                                    std::slice::from_ref(&historical),
                                    (&live_id, std::slice::from_ref(&live), live_depth),
                                    None,
                                )?,
                            );
                        } else {
                            children.push(SoundClockChildPair {
                                historical,
                                live,
                                live_parent: live_id.clone(),
                                historical_parent: None,
                                live_depth: live_depth + 1,
                            });
                        }
                    }
                    children
                };
                if let FrozenAudioKind::Repeat { iterations, .. } = &live_node.kind {
                    let historical_repeat = scoped_repeat(self, &historical_id)?;
                    let live_repeat = scoped_repeat(current, &live_id)?;
                    // The full sparse branch shape is already proven by the paired
                    // child list; these compact descriptors validate later paths.
                    debug_assert_eq!(&live_repeat.iterations, iterations);
                    debug_assert_eq!(&historical_repeat.iterations, iterations);
                    live_repeats.insert(live_id.clone(), live_repeat);
                    historical_repeats.insert(historical_id.clone(), historical_repeat);
                }
                let mut next = Vec::with_capacity(children.len());
                // Register the literal branch before its certified copies, then
                // preserve that same traversal order on the work stack.
                for child in children {
                    if isolation_state.aliases.get(&live_id) == Some(&historical_id) {
                        spend(&mut work, 1, maximum_work)?;
                        if isolation_state
                            .aliases
                            .insert(child.live.clone(), child.historical.clone())
                            .is_some_and(|previous| previous != child.historical)
                        {
                            return Err(invalid("sound clock isolation descendant alias changed"));
                        }
                    }
                    // A certified full context includes every descendant, including
                    // sparse Repeat branches. Only this recursive route permits
                    // several fresh copies to share one historical subtree.
                    if split_state.aliases.get(&live_id) == Some(&historical_id) {
                        spend(&mut work, 1, maximum_work)?;
                        if split_state
                            .aliases
                            .insert(child.live.clone(), child.historical.clone())
                            .is_some_and(|previous| previous != child.historical)
                        {
                            return Err(invalid("sound clock Split descendant alias changed"));
                        }
                    }
                    let historical_parent = child
                        .historical_parent
                        .unwrap_or_else(|| historical_id.clone());
                    let previous_parent = historical_parents
                        .insert(child.historical.clone(), historical_parent.clone());
                    if previous_parent.is_some_and(|previous| {
                        (split_state.aliases.get(&child.live) != Some(&child.historical)
                            && isolation_state.aliases.get(&child.live) != Some(&child.historical))
                            || previous != historical_parent
                    }) || live_parents
                        .insert(child.live.clone(), child.live_parent)
                        .is_some()
                    {
                        return Err(invalid("sound clock subtree contains a shared child"));
                    }
                    next.push((child.historical, child.live, child.live_depth));
                }
                pending.extend(next.into_iter().rev());
            }
            if visited_groups.len() != introduced_groups.len() {
                return Err(invalid(
                    "introduced sound clock group is outside the paired subtree",
                ));
            }
            if !split_state.complete() {
                return Err(invalid(
                    "sound clock Split relation is outside the paired subtree",
                ));
            }
            if !isolation_state.complete() {
                return Err(invalid(
                    "sound clock isolation is outside the paired subtree",
                ));
            }

            let historical_top = path_top(historical_scope, &historical_parents);
            let live_top = path_top(live_scope, &live_parents);
            Ok(SoundClockCorrespondence {
                inner: Arc::new(SoundClockCorrespondenceInner {
                    live_to_historical,
                    split_aliases: split_state.aliases,
                    introduced,
                    historical_placement_repeats: historical_ancestors.into_iter().collect(),
                    historical_path: ScopedPathIndex {
                        scope: historical_top,
                        parents: historical_parents,
                        repeats: historical_repeats,
                    },
                    live_path: ScopedPathIndex {
                        scope: live_top,
                        parents: live_parents,
                        repeats: live_repeats,
                    },
                    work,
                }),
            })
        })();
        (result, work)
    }

    /// Smallest scope that retains every processor above this independent owner.
    /// Ordinary Sequences and Repeats outside it affect placement only.
    pub(crate) fn sound_processing_scope(
        &self,
        owner: &NodeId,
        maximum_work: usize,
    ) -> Result<(NodeId, usize), DocumentError> {
        if !self.nodes.contains_key(owner) {
            return Err(invalid("sound owner is missing"));
        }
        let mut scope = owner.clone();
        let mut node = owner;
        let mut work = 0;
        while node != &self.root {
            spend(&mut work, 1, maximum_work)?;
            let (parent, _) = &self.index.parents[node];
            if matches!(self.nodes[parent].kind, FrozenAudioKind::Retime { purpose, .. }
                if purpose != RetimePurpose::Partition)
            {
                scope = parent.clone();
            }
            node = parent;
        }
        Ok((scope, work))
    }

    /// The retained structural parent, including explicit body and gap overrides.
    /// This address lookup grants no processing history or source admission.
    pub fn sound_parent(&self, node: &NodeId) -> Option<&NodeId> {
        self.index.parents.get(node).map(|(parent, _)| parent)
    }

    pub(crate) fn sound_placement_repeats(
        &self,
        scope: &NodeId,
        maximum_work: usize,
    ) -> Result<(Vec<NodeId>, usize), DocumentError> {
        let mut work = 0;
        let ancestors = placement_path(
            self,
            scope,
            false,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            maximum_work,
            &mut work,
        )?;
        Ok((ancestors, work))
    }

    /// Compare the compact placement path, without expanding its plays. A
    /// change after this scope in an ordinary Sequence needs no new clock.
    /// Processing equality is established separately by the scoped proof.
    pub(crate) fn sound_placement_unchanged(
        &self,
        current: &Self,
        scope: &NodeId,
        maximum_work: usize,
    ) -> Result<(bool, usize), DocumentError> {
        let mut work = 0;
        let mut child = scope;
        while child != &self.root {
            spend(&mut work, 1, maximum_work)?;
            let Some((old_parent, old_offset)) = self.index.parents.get(child) else {
                return Err(invalid("sound placement path is missing"));
            };
            let Some((parent, offset)) = current.index.parents.get(child) else {
                return Ok((false, work));
            };
            if old_parent != parent || old_offset != offset {
                return Ok((false, work));
            }
            match (&self.nodes[parent].kind, &current.nodes[parent].kind) {
                (FrozenAudioKind::Sequence { .. }, FrozenAudioKind::Sequence { .. }) => {}
                (
                    FrozenAudioKind::Retime {
                        purpose: RetimePurpose::Partition,
                        ..
                    },
                    FrozenAudioKind::Retime {
                        purpose: RetimePurpose::Partition,
                        ..
                    },
                ) => {
                    if self.nodes[parent] != current.nodes[parent]
                        || self.derived_clocks.get(parent) != current.derived_clocks.get(parent)
                    {
                        return Ok((false, work));
                    }
                }
                (
                    FrozenAudioKind::Repeat {
                        child: body,
                        iterations,
                        ..
                    },
                    FrozenAudioKind::Repeat {
                        iterations: live_iterations,
                        ..
                    },
                ) => {
                    let old_overrides = self.overrides.get(parent);
                    let new_overrides = current.overrides.get(parent);
                    let old_gaps = self.gap_overrides.get(parent);
                    let new_gaps = current.gap_overrides.get(parent);
                    spend(
                        &mut work,
                        iterations.segment_count()
                            + live_iterations.segment_count()
                            + old_overrides.map_or(0, PlayOverrides::len)
                            + new_overrides.map_or(0, PlayOverrides::len)
                            + old_gaps.map_or(0, PlayOverrides::len)
                            + new_gaps.map_or(0, PlayOverrides::len),
                        maximum_work,
                    )?;
                    if self.nodes[parent] != current.nodes[parent]
                        || old_overrides != new_overrides
                        || old_gaps != new_gaps
                    {
                        return Ok((false, work));
                    }
                    // Equal total Repeat duration does not prove equal play
                    // positions when independent override lengths changed.
                    for root in std::iter::once(body)
                        .chain(
                            old_overrides
                                .into_iter()
                                .flat_map(|entries| entries.iter().map(|(_, root)| root)),
                        )
                        .chain(
                            old_gaps
                                .into_iter()
                                .flat_map(|entries| entries.iter().map(|(_, root)| root)),
                        )
                    {
                        spend(&mut work, 1, maximum_work)?;
                        if self.node_extent(root)? != current.node_extent(root)? {
                            return Ok((false, work));
                        }
                    }
                }
                _ => return Err(invalid("sound placement path omits a processor")),
            }
            child = parent;
        }
        Ok((child == &current.root, work))
    }

    /// A surviving owner without explicit Split provenance must preserve its
    /// complete path through existing crops. Capture/import and recorded Split
    /// aliases have their own exact delivery routes. Ignore placement above the
    /// last crop so moving the copied fragment as a whole remains supported.
    pub(crate) fn sound_placement_crop_unchanged(
        &self,
        current: &Self,
        old_scope: &NodeId,
        live_scope: &NodeId,
        maximum_work: usize,
    ) -> Result<(bool, usize), DocumentError> {
        #[derive(PartialEq, Eq)]
        struct Step<'a> {
            child: &'a NodeId,
            offset: ExactRatio,
            duration: FrameDuration,
            kind: &'a FrozenAudioKind,
            clock: Option<&'a FrozenRetimeClock>,
        }
        fn path<'a>(
            layout: &'a FrozenAudioLayout,
            scope: &'a NodeId,
            work: &mut usize,
            maximum: usize,
        ) -> Result<Vec<Step<'a>>, DocumentError> {
            let mut child = scope;
            let mut path = Vec::new();
            let mut retained = 0;
            while child != layout.root() {
                spend(work, 1, maximum)?;
                let (parent, offset) = layout
                    .index
                    .parents
                    .get(child)
                    .ok_or_else(|| invalid("sound placement path is missing"))?;
                let node = &layout.nodes[parent];
                // Equality of compact kinds can examine children and Repeat
                // segments. Charge that comparison before retaining the entry.
                spend(
                    work,
                    match &node.kind {
                        FrozenAudioKind::Sequence { children } => children.len(),
                        FrozenAudioKind::Repeat { iterations, .. } => iterations.segment_count(),
                        _ => 1,
                    },
                    maximum,
                )?;
                path.push(Step {
                    child,
                    offset: *offset,
                    duration: node.duration,
                    kind: &node.kind,
                    clock: layout.derived_clocks.get(parent),
                });
                if matches!(
                    node.kind,
                    FrozenAudioKind::Retime {
                        purpose: RetimePurpose::Partition,
                        ..
                    }
                ) {
                    retained = path.len();
                }
                child = parent;
            }
            path.truncate(retained);
            Ok(path)
        }
        let mut work = 0;
        let before = path(self, old_scope, &mut work, maximum_work)?;
        let after = path(current, live_scope, &mut work, maximum_work)?;
        Ok((before == after, work))
    }

    /// Compatibility helper for callers which have not yet named a narrower
    /// scope. New code should retain the returned paired-scope proof.
    pub fn validate_sound_clock_owner(
        &self,
        current: &Self,
        owner: &NodeId,
        maximum_work: usize,
    ) -> Result<usize, DocumentError> {
        let (historical_scope, old_work) = self.branch_below(&self.root, owner, maximum_work)?;
        let remaining = maximum_work
            .checked_sub(old_work)
            .filter(|remaining| *remaining > 0)
            .ok_or_else(|| limit("sound clock comparison work exhausted"))?;
        let (live_scope, live_work) = current.branch_below(&current.root, owner, remaining)?;
        let prefix_work = old_work
            .checked_add(live_work)
            .ok_or_else(|| limit("sound clock comparison work overflow"))?;
        let remaining = maximum_work
            .checked_sub(prefix_work)
            .filter(|remaining| *remaining > 0)
            .ok_or_else(|| limit("sound clock comparison work exhausted"))?;
        let proof =
            self.sound_clock_correspondence(current, &historical_scope, &live_scope, remaining)?;
        if proof.historical_node(owner) != Some(owner) {
            return Err(invalid("sound clock owner changed its historical identity"));
        }
        prefix_work
            .checked_add(proof.work())
            .ok_or_else(|| limit("sound clock comparison work overflow"))
    }

    pub(crate) fn children<'a>(
        &'a self,
        id: &NodeId,
    ) -> impl DoubleEndedIterator<Item = &'a NodeId> {
        self.nodes[id]
            .kind
            .children()
            .iter()
            .chain(
                self.overrides
                    .get(id)
                    .into_iter()
                    .flat_map(|entries| entries.iter().map(|(_, child)| child)),
            )
            .chain(
                self.gap_overrides
                    .get(id)
                    .into_iter()
                    .flat_map(|entries| entries.iter().map(|(_, child)| child)),
            )
    }

    fn build_index(&self) -> Result<FrozenIndex, DocumentError> {
        if self.nodes.len() > MAX_DOCUMENT_NODES
            || self.overrides.len() > MAX_DOCUMENT_NODES
            || self.gap_overrides.len() > MAX_DOCUMENT_NODES
            || self.audio_lineage.len() > MAX_DOCUMENT_NODES
        {
            return Err(limit("frozen audio layout exceeds node limit"));
        }
        if self
            .audio_lineage
            .keys()
            .any(|id| !self.nodes.contains_key(id))
        {
            return Err(invalid("frozen audio lineage owner is missing"));
        }
        if !matches!(
            self.nodes.get(&self.root).map(|node| &node.kind),
            Some(FrozenAudioKind::Sequence { .. })
        ) {
            return Err(invalid("frozen audio root must be a Sequence"));
        }
        for (owner, entries) in self.overrides.iter().chain(&self.gap_overrides) {
            if entries.is_empty()
                || !matches!(
                    self.nodes.get(owner).map(|node| &node.kind),
                    Some(FrozenAudioKind::Repeat { .. })
                )
            {
                return Err(invalid("frozen overrides require an existing Repeat owner"));
            }
        }
        let mut edges = 0usize;
        let mut runs = 0usize;
        for id in self.nodes.keys() {
            edges = edges
                .checked_add(self.children(id).count())
                .ok_or_else(|| limit("frozen edge count overflow"))?;
            if edges > MAX_DOCUMENT_NODES {
                return Err(limit("frozen audio layout exceeds edge limit"));
            }
            if let FrozenAudioKind::Repeat { iterations, .. } = &self.nodes[id].kind {
                runs = runs
                    .checked_add(iterations.segment_count())
                    .ok_or_else(|| limit("frozen compact run count overflow"))?;
                if runs > MAX_FROZEN_AUDIO_RUNS {
                    return Err(limit("frozen compact run limit exceeded"));
                }
            }
        }
        let mut seen = crate::id_hash::id_set(self.nodes.len());
        let mut stack = vec![(&self.root, 0usize, false)];
        let mut durations: crate::id_hash::IdMap<&NodeId, FrameDuration> =
            crate::id_hash::id_map(self.nodes.len());
        let mut index = FrozenIndex {
            parents: crate::id_hash::id_map(self.nodes.len()),
            ..FrozenIndex::default()
        };
        let mut postorder = Vec::with_capacity(self.nodes.len());
        while let Some((id, depth, visited)) = stack.pop() {
            if depth > MAX_DOCUMENT_DEPTH {
                return Err(limit("frozen audio depth exceeds limit"));
            }
            let node = self
                .nodes
                .get(id)
                .ok_or_else(|| invalid("frozen audio child is missing"))?;
            if !visited {
                if !seen.insert(id) {
                    return Err(invalid("frozen audio has a cycle or shared child"));
                }
                validate_node(node)?;
                stack.push((id, depth, true));
                for child in self.children(id).rev() {
                    stack.push((child, depth + 1, false));
                }
                continue;
            }
            let computed = match &node.kind {
                FrozenAudioKind::Sequence { children } => {
                    let mut sum = FrameDuration::ZERO;
                    for child in children {
                        sum = sum.checked_add(durations[child])?;
                    }
                    sum
                }
                FrozenAudioKind::Repeat {
                    child,
                    iterations,
                    gap_duration,
                    ..
                } => {
                    // The layout reads only these evaluated branch roots.
                    let branches: BTreeMap<NodeId, FrameDuration> = self
                        .children(id)
                        .filter_map(|branch| {
                            durations
                                .get(branch)
                                .map(|duration| (branch.clone(), *duration))
                        })
                        .collect();
                    let repeat = RepeatLayout::compile_with_gap_overrides(
                        iterations,
                        child,
                        self.overrides.get(id),
                        *gap_duration,
                        self.gap_overrides.get(id),
                        &branches,
                    )?;
                    let duration = repeat.duration();
                    index.repeats.insert(id.clone(), repeat.into());
                    duration
                }
                FrozenAudioKind::Retime { child, mapping, .. } => {
                    if mapping.start().0 < 0
                        || mapping.duration() == FrameDuration::ZERO
                        || mapping.end().0 > durations[child].frames()
                    {
                        return Err(invalid("frozen retime mapping is outside its child"));
                    }
                    node.duration
                }
                _ => node.duration,
            };
            if computed != node.duration {
                return Err(invalid("frozen duration disagrees with its structure"));
            }
            durations.insert(id, computed);
            postorder.push(id);
            let mut offset = ExactRatio::ZERO;
            for child in self.children(id) {
                index.parents.insert(child.clone(), (id.clone(), offset));
                if matches!(node.kind, FrozenAudioKind::Sequence { .. }) {
                    offset = offset.checked_add(ExactRatio::integer(durations[child].frames()))?;
                }
            }
        }
        if seen.len() != self.nodes.len() {
            return Err(invalid("frozen audio contains unreachable nodes"));
        }
        self.index_exact_clocks(&mut index, &postorder)?;
        Ok(index)
    }

    /// Project a complete frozen occurrence without consulting live structure.
    /// Local coordinates may be outside a visible crop or host: source placement
    /// and full retained contexts need this affine continuation, without clamping.
    /// Gap scopes name the Repeat itself and only its outer Repeat ancestors.
    pub fn project(
        &self,
        instance: &InstancePath,
        local: ExactRatio,
        gap_after: Option<&IterationId>,
        maximum_work: usize,
    ) -> Result<FrozenAudioProjection, DocumentError> {
        self.project_scoped(&self.root, instance, local, gap_after, maximum_work)
    }

    /// Project an occurrence relative to an explicit lexical definition root.
    /// The Repeat path contains only ancestors strictly inside that scope;
    /// no enclosing occurrence or current alias is inferred.
    pub fn project_scoped(
        &self,
        root: &NodeId,
        instance: &InstancePath,
        local: ExactRatio,
        gap_after: Option<&IterationId>,
        maximum_work: usize,
    ) -> Result<FrozenAudioProjection, DocumentError> {
        self.project_scoped_inner(
            root,
            instance,
            local,
            gap_after,
            maximum_work,
            ProjectionMode::Affine,
        )
        .map(|(projection, _)| projection)
    }

    /// Project local zero and meaningful local support within one physical
    /// audio clock. An ancestor nonunity Preserve is a clock boundary: use its
    /// input child as the scope, or project the Preserve output itself.
    /// Ordinary Edit crops constrain support; transparent Partitions retain it.
    /// A gap names its owning Repeat and the stable play immediately before it,
    /// with only outer Repeat occurrences in `instance.repeats`. Its intrinsic
    /// support is the actual gap duration, never the whole Repeat duration.
    pub fn project_scoped_with_support(
        &self,
        root: &NodeId,
        instance: &InstancePath,
        gap_after: Option<&IterationId>,
        maximum_work: usize,
    ) -> Result<(FrozenAudioProjection, std::ops::Range<ExactRatio>), DocumentError> {
        self.project_scoped_inner(
            root,
            instance,
            ExactRatio::ZERO,
            gap_after,
            maximum_work,
            ProjectionMode::MeaningfulSupport,
        )
    }

    /// Project local zero and the physical local allocation visible within one
    /// lexical scope. Every ancestor Retime selection constrains the allocation,
    /// including transparent Partitions. An empty or disjoint allocation is None.
    /// The affine origin is retained even when it lies outside the visible scope.
    /// As with meaningful support, crossing a nonunity Preserve is rejected.
    /// Gap occurrences retain their stable preceding play and actual gap duration.
    pub fn project_scoped_with_allocation(
        &self,
        root: &NodeId,
        instance: &InstancePath,
        gap_after: Option<&IterationId>,
        maximum_work: usize,
    ) -> Result<(FrozenAudioProjection, Option<std::ops::Range<ExactRatio>>), DocumentError> {
        let (projection, allocation) = self.project_scoped_inner(
            root,
            instance,
            ExactRatio::ZERO,
            gap_after,
            maximum_work,
            ProjectionMode::VisibleAllocation,
        )?;
        Ok((
            projection,
            (allocation.start != allocation.end).then_some(allocation),
        ))
    }

    fn project_scoped_inner(
        &self,
        root: &NodeId,
        instance: &InstancePath,
        local: ExactRatio,
        gap_after: Option<&IterationId>,
        maximum_work: usize,
        mode: ProjectionMode,
    ) -> Result<(FrozenAudioProjection, std::ops::Range<ExactRatio>), DocumentError> {
        let value = legacy(self.project_scoped_inner_admitted(
            ScopedProjectionRequest {
                root,
                instance,
                local,
                gap_after,
                maximum_work,
                mode,
            },
            &mut Unmetered,
        ))?;
        Ok(value.into_legacy())
    }

    pub(crate) fn project_scoped_inner_admitted<P: AudioBindingAdmission>(
        &self,
        request: ScopedProjectionRequest<'_>,
        admission: &mut P,
    ) -> AdmittedResult<ProjectedSupport, P> {
        let ScopedProjectionRequest {
            root,
            instance,
            local,
            gap_after,
            maximum_work,
            mode,
        } = request;
        if maximum_work == 0 || maximum_work > MAX_DOCUMENT_NODES {
            return Err(limit("invalid frozen projection budget").into());
        }
        instance.validate_depth()?;
        if !self.has_node(root) {
            return Err(invalid("frozen projection scope is missing").into());
        }
        let target = self
            .nodes
            .get(&instance.node)
            .ok_or_else(|| invalid("frozen projection host is missing"))?;
        let mut raw = Work {
            used: 0,
            maximum: maximum_work,
        };
        let mut extra = 0;
        let mut work = Build::projection(&mut raw, &mut extra, admission);
        let mut origin = ExactRatio::ZERO;
        let mut scale = ExactRatio::ONE;
        let mut local_duration = target.duration;
        let mut local_extent = self.node_extent(&instance.node)?;
        if let Some(gap) = gap_after {
            let layout = self
                .index
                .repeats
                .get(&instance.node)
                .ok_or_else(|| invalid("frozen gap scope is not a Repeat"))?;
            work.spend(layout.segment_count())?;
            let play = layout
                .play(gap)?
                .ok_or_else(|| invalid("frozen gap play is missing"))?;
            if play.gap_after == ExactRatio::ZERO || play.gap_child.is_some() {
                return Err(invalid("frozen play has no following gap").into());
            }
            origin = play.start.checked_add(play.duration)?;
            // A default gap remains an integral authored recipe; owned gaps
            // are independent nodes and never enter this branch.
            local_duration = crate::derived_clock::integral(play.gap_after)?;
            local_extent = play.gap_after;
        }
        let mut support = ExactRatio::ZERO..local_extent;
        let mut node = &instance.node;
        let mut step = instance.repeats.len();
        loop {
            work.spend(1)?;
            if node == root {
                break;
            }
            let (parent, offset) = self
                .index
                .parents
                .get(node)
                .ok_or_else(|| invalid("frozen projection host is outside scope"))?;
            // The admitted root is a Sequence; skip looking it up.
            if parent == &self.root {
                origin = origin.checked_add(*offset)?;
                node = parent;
                continue;
            }
            match &self.nodes[parent].kind {
                FrozenAudioKind::Sequence { .. } => origin = origin.checked_add(*offset)?,
                FrozenAudioKind::Retime {
                    mapping,
                    pitch,
                    purpose,
                    ..
                } => {
                    let clock = self.retime_clock(parent)?;
                    if mode != ProjectionMode::Affine
                        && pitch.processes(mapping.duration() == self.nodes[parent].duration)
                    {
                        return Err(
                            invalid("frozen support scope crosses an opaque Preserve").into()
                        );
                    }
                    if mode == ProjectionMode::VisibleAllocation
                        || (mode == ProjectionMode::MeaningfulSupport
                            && *purpose != RetimePurpose::Partition)
                    {
                        let selected = clock.child_start.checked_sub(origin)?.checked_div(scale)?
                            ..clock.child_end.checked_sub(origin)?.checked_div(scale)?;
                        clip_binding_support(&mut support, selected)?;
                    }
                    let factor = clock
                        .extent
                        .checked_div(clock.child_end.checked_sub(clock.child_start)?)?;
                    origin = origin.checked_sub(clock.child_start)?.checked_mul(factor)?;
                    scale = scale.checked_mul(factor)?;
                }
                FrozenAudioKind::Repeat { .. } => {
                    step = step
                        .checked_sub(1)
                        .ok_or_else(|| invalid("frozen occurrence omits a Repeat"))?;
                    let selected = &instance.repeats[step];
                    if &selected.node != parent {
                        return Err(invalid("frozen Repeat path order is wrong").into());
                    }
                    let layout = &self.index.repeats[parent];
                    work.spend(layout.segment_count())?;
                    let play = layout
                        .play(&selected.iteration)?
                        .ok_or_else(|| invalid("frozen occurrence play is missing"))?;
                    let offset = if &play.child == node {
                        play.start
                    } else if play.gap_child.as_ref() == Some(node) {
                        // A retained dormant branch still has an affine clock
                        // and meaningful recipe; it contributes no allocation.
                        if mode == ProjectionMode::VisibleAllocation
                            && play.gap_after == ExactRatio::ZERO
                        {
                            support.end = support.start;
                        }
                        play.start.checked_add(play.duration)?
                    } else {
                        return Err(
                            invalid("frozen occurrence selects the wrong override child").into(),
                        );
                    };
                    origin = origin.checked_add(offset)?;
                }
                _ => return Err(invalid("frozen projection has a leaf parent").into()),
            }
            node = parent;
        }
        if step != 0 {
            return Err(invalid("frozen occurrence has extra Repeat ancestors").into());
        }
        let result = (
            FrozenAudioProjection {
                origin,
                frames_per_local_frame: scale,
                point: origin.checked_add(local.checked_mul(scale)?)?,
                local_duration,
                local_extent,
                instance: work.instance(instance)?,
                gap_after: gap_after.map(|v| work.iteration(v)).transpose()?,
                work: work.used(),
            },
            support,
        );
        Ok(work.finish(result))
    }

    /// Whether this node's complete placement path contains only ordinary
    /// Sequences. Uses the admitted parent index, including override roots.
    pub(crate) fn ordinary_sound_ancestry(
        &self,
        target: &NodeId,
        maximum_work: usize,
    ) -> Result<(bool, usize), DocumentError> {
        if maximum_work == 0 || maximum_work > MAX_DOCUMENT_NODES {
            return Err(limit("invalid sound grouping ancestry budget"));
        }
        if !self.nodes.contains_key(target) {
            return Err(invalid("sound grouping owner is missing"));
        }
        let mut node = target;
        let mut work = 0;
        loop {
            spend(&mut work, 1, maximum_work)?;
            if self.derived_clocks.contains_key(node) {
                return Ok((false, work));
            }
            if node == &self.root {
                return Ok((true, work));
            }
            let (parent, _) = self
                .index
                .parents
                .get(node)
                .ok_or_else(|| invalid("sound grouping owner ancestry is missing"))?;
            if !matches!(self.nodes[parent].kind, FrozenAudioKind::Sequence { .. }) {
                return Ok((false, work));
            }
            node = parent;
        }
    }

    pub(crate) fn sound_scope_contains(
        &self,
        scope: &NodeId,
        target: &NodeId,
        maximum_work: usize,
    ) -> Result<(bool, usize), DocumentError> {
        if !self.nodes.contains_key(scope) || !self.nodes.contains_key(target) {
            return Ok((false, 0));
        }
        let mut node = target;
        let mut work = 0;
        loop {
            spend(&mut work, 1, maximum_work)?;
            if node == scope {
                return Ok((true, work));
            }
            if node == &self.root {
                return Ok((false, work));
            }
            node = &self.index.parents[node].0;
        }
    }

    /// Immediate owned child on the target's path below an explicit ancestor.
    /// Sparse overrides are indexed parents, so unrelated branches cost no work.
    pub(crate) fn branch_below(
        &self,
        ancestor: &NodeId,
        target: &NodeId,
        maximum_work: usize,
    ) -> Result<(NodeId, usize), DocumentError> {
        let mut value =
            legacy(self.branch_below_admitted(ancestor, target, maximum_work, &mut Unmetered))?;
        let work = legacy(value.take_work(&Unmetered))?.units();
        Ok((value.into_legacy(), work))
    }

    pub(crate) fn branch_below_admitted<P: AudioBindingAdmission>(
        &self,
        ancestor: &NodeId,
        target: &NodeId,
        maximum_work: usize,
        admission: &mut P,
    ) -> AdmittedResult<NodeId, P> {
        if maximum_work == 0 || maximum_work > MAX_DOCUMENT_NODES {
            return Err(limit("invalid frozen branch budget").into());
        }
        if !self.nodes.contains_key(ancestor) || !self.nodes.contains_key(target) {
            return Err(invalid("frozen branch alias is missing").into());
        }
        let mut node = target;
        let mut raw = Work {
            used: 0,
            maximum: maximum_work,
        };
        let mut extra = 0;
        let mut work = Build::projection(&mut raw, &mut extra, admission);
        loop {
            work.spend(1)?;
            let (parent, _) = self
                .index
                .parents
                .get(node)
                .ok_or_else(|| invalid("frozen ancestor does not contain its target"))?;
            if parent == ancestor {
                let node = work.node(node)?;
                return Ok(work.finish(node));
            }
            node = parent;
        }
    }
}

/// Where a walked placement path ends: the root, or just below a processor.
fn path_top(scope: &NodeId, parents: &BTreeMap<NodeId, NodeId>) -> NodeId {
    let mut node = scope;
    while let Some(parent) = parents.get(node) {
        node = parent;
    }
    node.clone()
}

/// The nearest Retime above `scope` that processes rather than partitions.
fn enclosing_processor<'a>(layout: &'a FrozenAudioLayout, scope: &'a NodeId) -> Option<&'a NodeId> {
    let mut node = scope;
    while let Some((parent, _)) = layout.index.parents.get(node) {
        if matches!(
            layout.nodes[parent].kind,
            FrozenAudioKind::Retime { purpose, .. } if purpose != RetimePurpose::Partition
        ) {
            return Some(parent);
        }
        node = parent;
    }
    None
}

fn placement_path(
    layout: &FrozenAudioLayout,
    scope: &NodeId,
    inside_processor: bool,
    parents: &mut BTreeMap<NodeId, NodeId>,
    repeats: &mut BTreeMap<NodeId, ScopedRepeat>,
    maximum_work: usize,
    work: &mut usize,
) -> Result<Vec<NodeId>, DocumentError> {
    let mut node = scope;
    let mut ancestors = Vec::new();
    while node != &layout.root {
        let (parent, _) = layout
            .index
            .parents
            .get(node)
            .ok_or_else(|| invalid("sound clock scope is outside its project root"))?;
        spend(work, 1, maximum_work)?;
        match &layout.nodes[parent].kind {
            FrozenAudioKind::Sequence { .. } => {}
            FrozenAudioKind::Retime {
                purpose: RetimePurpose::Partition,
                ..
            } => {}
            FrozenAudioKind::Repeat { iterations, .. } => {
                spend(
                    work,
                    1 + iterations.segment_count()
                        + layout.overrides.get(parent).map_or(0, PlayOverrides::len)
                        + layout
                            .gap_overrides
                            .get(parent)
                            .map_or(0, PlayOverrides::len),
                    maximum_work,
                )?;
                repeats.insert(parent.clone(), scoped_repeat(layout, parent)?);
                ancestors.push(parent.clone());
            }
            FrozenAudioKind::Retime { .. } if inside_processor => break,
            _ => return Err(invalid("sound clock scope omits an enclosing processor")),
        }
        parents.insert(node.clone(), parent.clone());
        node = parent;
    }
    ancestors.reverse();
    Ok(ancestors)
}

fn scoped_repeat(layout: &FrozenAudioLayout, id: &NodeId) -> Result<ScopedRepeat, DocumentError> {
    let FrozenAudioKind::Repeat {
        child, iterations, ..
    } = &layout
        .nodes
        .get(id)
        .ok_or_else(|| invalid("sound clock Repeat is missing"))?
        .kind
    else {
        return Err(invalid("sound clock Repeat pair changed kind"));
    };
    Ok(ScopedRepeat {
        iterations: iterations.clone(),
        child: child.clone(),
        overrides: layout.overrides.get(id).cloned().unwrap_or_default(),
        gap_overrides: layout.gap_overrides.get(id).cloned().unwrap_or_default(),
    })
}

/// Count work over both sides before the comparison scans compact Repeat
/// identity runs, override keys, or copies any child/path indexes.
fn sound_clock_pair_work(
    historical: &FrozenAudioLayout,
    current: &FrozenAudioLayout,
    historical_id: &NodeId,
    live_id: &NodeId,
) -> Result<usize, DocumentError> {
    fn node_work(layout: &FrozenAudioLayout, id: &NodeId) -> Result<usize, DocumentError> {
        let node = layout
            .nodes
            .get(id)
            .ok_or_else(|| invalid("sound clock node is missing"))?;
        let work = match &node.kind {
            FrozenAudioKind::Sequence { children } => children.len(),
            FrozenAudioKind::Repeat { iterations, .. } => 1usize
                .checked_add(iterations.segment_count())
                .and_then(|count| {
                    count.checked_add(layout.overrides.get(id).map_or(0, PlayOverrides::len))
                })
                .and_then(|count| {
                    count.checked_add(layout.gap_overrides.get(id).map_or(0, PlayOverrides::len))
                })
                .ok_or_else(|| limit("sound clock Repeat work overflow"))?,
            FrozenAudioKind::Retime { .. } => 1,
            FrozenAudioKind::Source { .. } | FrozenAudioKind::Hold { .. } => 0,
        };
        Ok(work)
    }

    node_work(historical, historical_id)?
        .checked_add(node_work(current, live_id)?)
        .ok_or_else(|| limit("sound clock pair work overflow"))
}

struct SoundClockChildPair {
    historical: NodeId,
    historical_parent: Option<NodeId>,
    live: NodeId,
    live_parent: NodeId,
    live_depth: usize,
}

fn paired_sound_clock_children(
    historical: &FrozenAudioLayout,
    current: &FrozenAudioLayout,
    historical_id: &NodeId,
    live_id: &NodeId,
) -> Result<Vec<(NodeId, NodeId)>, DocumentError> {
    let historical_node = &historical.nodes[historical_id];
    let live_node = &current.nodes[live_id];
    match (&historical_node.kind, &live_node.kind) {
        (
            FrozenAudioKind::Sequence {
                children: historical,
            },
            FrozenAudioKind::Sequence { children: live },
        ) if historical.len() == live.len() => Ok(historical
            .iter()
            .cloned()
            .zip(live.iter().cloned())
            .collect()),
        (
            FrozenAudioKind::Repeat {
                child: historical_child,
                iterations: historical_order,
                ..
            },
            FrozenAudioKind::Repeat {
                child: live_child,
                iterations: live_order,
                ..
            },
        ) if historical_order == live_order => {
            let historical_overrides = historical.overrides.get(historical_id);
            let live_overrides = current.overrides.get(live_id);
            let historical_gaps = historical.gap_overrides.get(historical_id);
            let live_gaps = current.gap_overrides.get(live_id);
            if !same_override_keys(historical_overrides, live_overrides)
                || !same_override_keys(historical_gaps, live_gaps)
            {
                return Err(invalid("sound clock Repeat override keys changed"));
            }
            let mut children = vec![(historical_child.clone(), live_child.clone())];
            if let (Some(old), Some(new)) = (historical_overrides, live_overrides) {
                children.extend(old.iter().zip(new.iter()).map(
                    |((_, old_child), (_, new_child))| (old_child.clone(), new_child.clone()),
                ));
            }
            if let (Some(old), Some(new)) = (historical_gaps, live_gaps) {
                children.extend(old.iter().zip(new.iter()).map(
                    |((_, old_child), (_, new_child))| (old_child.clone(), new_child.clone()),
                ));
            }
            Ok(children)
        }
        (
            FrozenAudioKind::Retime {
                child: historical_child,
                ..
            },
            FrozenAudioKind::Retime {
                child: live_child, ..
            },
        ) => Ok(vec![(historical_child.clone(), live_child.clone())]),
        (FrozenAudioKind::Source { .. }, FrozenAudioKind::Source { .. })
        | (FrozenAudioKind::Hold { .. }, FrozenAudioKind::Hold { .. }) => Ok(Vec::new()),
        _ => Err(invalid("sound clock processing subtree changed")),
    }
}

fn same_override_keys(historical: Option<&PlayOverrides>, live: Option<&PlayOverrides>) -> bool {
    match (historical, live) {
        (None, None) => true,
        (Some(historical), Some(live)) => {
            historical.len() == live.len()
                && historical
                    .iter()
                    .zip(live.iter())
                    .all(|((historical, _), (live, _))| historical == live)
        }
        _ => false,
    }
}

fn validate_scoped_instance(
    index: &ScopedPathIndex,
    instance: &InstancePath,
    maximum_work: usize,
) -> Result<usize, DocumentError> {
    validate_scoped_instance_presence(index, instance, &BTreeSet::new(), maximum_work)
        .map(|(_, work)| work)
}

fn validate_scoped_instance_presence(
    index: &ScopedPathIndex,
    instance: &InstancePath,
    optional: &BTreeSet<NodeId>,
    maximum_work: usize,
) -> Result<(bool, usize), DocumentError> {
    if maximum_work == 0 || maximum_work > MAX_DOCUMENT_NODES {
        return Err(limit("invalid sound clock path budget"));
    }
    instance.validate_depth()?;
    let mut node = &instance.node;
    let mut repeat_step = instance.repeats.len();
    let mut work = 0usize;
    let mut present = true;
    while node != &index.scope {
        spend(&mut work, 1, maximum_work)?;
        let parent = index
            .parents
            .get(node)
            .ok_or_else(|| invalid("sound clock occurrence is outside its scope"))?;
        if let Some(repeat) = index.repeats.get(parent) {
            repeat_step = repeat_step
                .checked_sub(1)
                .ok_or_else(|| invalid("sound clock occurrence omits a Repeat ancestor"))?;
            let selected = &instance.repeats[repeat_step];
            if &selected.node != parent {
                return Err(invalid("sound clock occurrence Repeat order is wrong"));
            }
            spend(&mut work, repeat.iterations.segment_count(), maximum_work)?;
            let position = repeat.iterations.position(&selected.iteration);
            let child = repeat
                .overrides
                .get(&selected.iteration)
                .unwrap_or(&repeat.child);
            let gap_child = repeat.gap_overrides.get(&selected.iteration);
            if position.is_none()
                || child != node && gap_child != Some(node)
                || gap_child == Some(node)
                    && position.is_some_and(|position| position + 1 == repeat.iterations.len())
            {
                if optional.contains(parent) {
                    present = false;
                } else {
                    return Err(invalid(
                        "sound clock occurrence selects an absent play or branch",
                    ));
                }
            }
        }
        node = parent;
    }
    if repeat_step != 0 {
        return Err(invalid("sound clock occurrence has extra Repeat ancestors"));
    }
    Ok((present, work))
}

fn same_sound_clock_kind(before: &FrozenAudioKind, after: &FrozenAudioKind) -> bool {
    match (before, after) {
        (FrozenAudioKind::Source { .. }, FrozenAudioKind::Source { .. })
        | (FrozenAudioKind::Hold { .. }, FrozenAudioKind::Hold { .. }) => true,
        (FrozenAudioKind::Sequence { children: a }, FrozenAudioKind::Sequence { children: b }) => {
            a.len() == b.len()
        }
        (
            FrozenAudioKind::Repeat {
                iterations: ai,
                gap_duration: ag,
                gap_audio: _,
                child: _,
            },
            FrozenAudioKind::Repeat {
                iterations: bi,
                gap_duration: bg,
                gap_audio: _,
                child: _,
            },
        ) => ai == bi && ag == bg,
        (
            FrozenAudioKind::Retime {
                mapping: am,
                pitch: ap,
                purpose: au,
                child: _,
            },
            FrozenAudioKind::Retime {
                mapping: bm,
                pitch: bp,
                purpose: bu,
                child: _,
            },
        ) => am == bm && ap == bp && au == bu,
        _ => false,
    }
}

pub(crate) fn clip_binding_support(
    support: &mut std::ops::Range<ExactRatio>,
    constraint: std::ops::Range<ExactRatio>,
) -> Result<(), TimeError> {
    let min = |a: ExactRatio, b: ExactRatio| -> Result<ExactRatio, TimeError> {
        Ok(if a.checked_sub(b)?.compare_integer(0).is_le() {
            a
        } else {
            b
        })
    };
    let max = |a: ExactRatio, b: ExactRatio| -> Result<ExactRatio, TimeError> {
        Ok(if a.checked_sub(b)?.compare_integer(0).is_ge() {
            a
        } else {
            b
        })
    };
    let start = min(max(support.start, constraint.start)?, support.end)?;
    let end = max(min(support.end, constraint.end)?, start)?;
    *support = start..end;
    Ok(())
}

/// The frozen form of one authored node. Without `children`, a Sequence's
/// child list is left empty (enough for `validate_node`, which never reads it).
fn frozen_node(
    node: &crate::BeatNode,
    duration: FrameDuration,
    rate: FrameRate,
    children: bool,
) -> Result<FrozenAudioNode, DocumentError> {
    let kind = match &node.kind {
        NodeKind::Source { source } => {
            let placement = source
                .audio
                .as_ref()
                .map(|_| {
                    source.audio_mapping.selection_frames_with_offset(
                        source.duration,
                        source.audio_offset,
                        rate,
                    )
                })
                .transpose()?;
            FrozenAudioKind::Source { placement }
        }
        NodeKind::Hold { recipe } => FrozenAudioKind::Hold {
            audio: (&recipe.audio).into(),
        },
        NodeKind::Sequence { children: list } => FrozenAudioKind::Sequence {
            children: if children { list.clone() } else { Vec::new() },
        },
        // Frozen timing omits postmapping escalation, like gain.
        NodeKind::Repeat {
            child,
            iterations,
            gap,
            ..
        } => FrozenAudioKind::Repeat {
            child: child.clone(),
            iterations: iterations.clone(),
            gap_duration: gap.as_ref().map_or(FrameDuration::ZERO, |gap| gap.duration),
            gap_audio: gap
                .as_ref()
                .map_or(ReferenceAudibility::Silence, |gap| (&gap.audio).into()),
        },
        NodeKind::Retime {
            child,
            mapping,
            pitch,
            purpose,
            ..
        } => FrozenAudioKind::Retime {
            child: child.clone(),
            mapping: *mapping,
            pitch: *pitch,
            purpose: *purpose,
        },
    };
    Ok(FrozenAudioNode {
        duration,
        edges: node.audio_edges,
        editorial_edges: node.audio_editorial_edges,
        kind,
    })
}

fn validate_node(node: &FrozenAudioNode) -> Result<(), DocumentError> {
    match node.kind {
        FrozenAudioKind::Hold { audio } => audio.validate_duration(node.duration)?,
        FrozenAudioKind::Repeat {
            gap_audio,
            gap_duration,
            ..
        } => gap_audio.validate_duration(gap_duration)?,
        _ => {}
    }
    if !matches!(
        node.kind,
        FrozenAudioKind::Sequence { .. } | FrozenAudioKind::Repeat { .. }
    ) && node.duration == FrameDuration::ZERO
    {
        return Err(invalid("frozen leaf/retime duration must be positive"));
    }
    if let FrozenAudioKind::Source {
        placement: Some(placement),
    } = &node.kind
    {
        if placement.start == placement.end {
            if placement.start.compare_integer(i64::MIN).is_lt()
                || placement.end.compare_integer(i64::MAX).is_gt()
            {
                return Err(crate::TimeError::Overflow.into());
            }
        } else {
            ExactFrameRange::new(placement.start, placement.end)?;
        }
    }
    if let FrozenAudioKind::Repeat {
        gap_duration: FrameDuration::ZERO,
        gap_audio,
        ..
    } = &node.kind
        && *gap_audio != ReferenceAudibility::Silence
    {
        return Err(invalid("empty frozen gap cannot contain audio"));
    }
    for boundary in [
        AudioBoundaryKind::SourcePlacementStart,
        AudioBoundaryKind::SourcePlacementEnd,
        AudioBoundaryKind::RepeatGapStart,
        AudioBoundaryKind::RepeatGapEnd,
    ] {
        let allowed = match boundary {
            AudioBoundaryKind::SourcePlacementStart | AudioBoundaryKind::SourcePlacementEnd => {
                matches!(node.kind, FrozenAudioKind::Source { .. })
            }
            _ => matches!(node.kind, FrozenAudioKind::Repeat { .. }),
        };
        if !allowed && node.edges.get(boundary) != AudioEdgePolicy::Automatic {
            return Err(invalid("frozen edge policy is unsupported by its node"));
        }
    }
    if let FrozenAudioKind::Retime {
        pitch: crate::PitchPolicy::Shift { semitones },
        purpose,
        ..
    } = &node.kind
        && (crate::PitchPolicy::shifted(*semitones).ok()
            != Some(crate::PitchPolicy::Shift {
                semitones: *semitones,
            })
            || *purpose == RetimePurpose::Partition)
    {
        return Err(invalid(
            "a frozen pitch shift is a nonzero whole number of semitones within ±24 on an ordinary Retime",
        ));
    }
    if let FrozenAudioKind::Retime {
        purpose: RetimePurpose::Partition,
        mapping,
        ..
    } = &node.kind
        && (node.duration != mapping.duration()
            || !node.editorial_edges.permits_partition_policies(node.edges))
    {
        return Err(invalid(
            "frozen Partition requires unity timing and Hard policies only on marked editorial sides",
        ));
    }
    Ok(())
}

fn spend(work: &mut usize, amount: usize, maximum: usize) -> Result<(), DocumentError> {
    *work = work
        .checked_add(amount)
        .filter(|next| *next <= maximum)
        .ok_or_else(|| limit("frozen projection work exhausted"))?;
    Ok(())
}
fn invalid(message: &str) -> DocumentError {
    DocumentError::new(DocumentErrorCode::InvalidTree, message)
}
fn limit(message: &str) -> DocumentError {
    DocumentError::new(DocumentErrorCode::LimitExceeded, message)
}

struct BoundedJson {
    bytes: Vec<u8>,
    exceeded: bool,
}
impl Write for BoundedJson {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > MAX_DOCUMENT_JSON_BYTES - self.bytes.len() {
            self.exceeded = true;
            return Err(std::io::Error::other(
                "frozen audio JSON exceeds byte limit",
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod group_correspondence_tests;
#[cfg(test)]
mod split_correspondence_tests;
