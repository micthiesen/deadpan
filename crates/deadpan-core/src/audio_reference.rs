//! Immutable timing and audibility facts. Aliases belong to this layout, never
//! to a later live document; no media, marks or previous bindings are retained.

mod preflight;

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::sync::Arc;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    },
    /// Source audio played backwards from the Hold's start.
    Reverse,
    /// A synthesized tone; no source input.
    Tone {
        frequency_hz: u32,
        level: crate::GainDb,
    },
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
    },
    Reverse {},
    Tone {
        frequency_hz: u32,
        level: crate::GainDb,
    },
}

impl<'de> Deserialize<'de> for ReferenceAudibility {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let audio = match AudibilityWire::deserialize(deserializer)? {
            AudibilityWire::Silence {} => Self::Silence,
            AudibilityWire::RoomTone {} => Self::RoomTone,
            AudibilityWire::Tail { maximum, effect } => Self::Tail { maximum, effect },
            AudibilityWire::Reverse {} => Self::Reverse,
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
                maximum, effect, ..
            } => Self::Tail {
                maximum: *maximum,
                effect: *effect,
            },
            HoldAudio::Reverse { .. } => Self::Reverse,
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
    pub edges: AudioEdgePolicies,
    #[serde(default, skip_serializing_if = "crate::AudioEditorialEdges::is_empty")]
    pub editorial_edges: crate::AudioEditorialEdges,
    pub kind: FrozenAudioKind,
}

#[derive(Debug, Clone, Default)]
struct FrozenIndex {
    parents: BTreeMap<NodeId, (NodeId, i64)>,
    repeats: BTreeMap<NodeId, RepeatLayout>,
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
    #[serde(skip)]
    index: FrozenIndex,
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
                && self.audio_lineage == other.audio_lineage)
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FrozenAudioProjection {
    pub origin: ExactRatio,
    pub frames_per_local_frame: ExactRatio,
    pub point: ExactRatio,
    pub local_duration: FrameDuration,
    pub instance: InstancePath,
    pub gap_after: Option<IterationId>,
    /// Charged nodes and compact identity segments, not rendered play count.
    pub work: usize,
}

/// A bounded, typed correspondence between two complete processing subtrees.
/// Node identities may differ, but clocks, Repeat identities, and processing
/// controls must agree. The proof owns only bounded scoped indexes, not either
/// full layout.
#[derive(Debug, Clone)]
pub struct SoundClockCorrespondence {
    inner: Arc<SoundClockCorrespondenceInner>,
}

#[derive(Debug)]
struct SoundClockCorrespondenceInner {
    live_to_historical: BTreeMap<NodeId, NodeId>,
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
    /// Work charged while proving the two complete scoped subtrees.
    pub fn work(&self) -> usize {
        self.inner.work
    }

    /// Historical node corresponding to a live node in the paired scope.
    pub fn historical_node(&self, live: &NodeId) -> Option<&NodeId> {
        self.inner.live_to_historical.get(live)
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
        if maximum_work == 0 || maximum_work > MAX_DOCUMENT_NODES {
            return Err(limit("invalid sound clock path budget"));
        }
        let live_work = validate_scoped_instance(&self.inner.live_path, live, maximum_work)?;
        let historical = InstancePath {
            node: self
                .historical_node(&live.node)
                .cloned()
                .ok_or_else(|| invalid("sound clock occurrence is outside its live scope"))?,
            repeats: live
                .repeats
                .iter()
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
        let historical_work =
            validate_scoped_instance(&self.inner.historical_path, &historical, remaining)?;
        let work = live_work
            .checked_add(historical_work)
            .ok_or_else(|| limit("sound clock occurrence work overflow"))?;
        Ok((historical, work))
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ProjectionMode {
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
    ) -> Result<Self, DocumentError> {
        Self::admit(LayoutWire {
            root,
            rate,
            nodes,
            overrides,
            gap_overrides,
            audio_lineage,
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
        // Precharge before cloning nodes, overrides or iteration vectors and
        // before durations() builds any derived Repeat layouts.
        let mut edges = 0usize;
        let mut runs = 0usize;
        if document.audio_lineage().len() > MAX_DOCUMENT_NODES {
            return Err(limit("frozen audio lineage exceeds node limit"));
        }
        for (id, node) in document.nodes() {
            edges = edges
                .checked_add(document.children(id).count())
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
        let durations = if validate_bindings {
            document.durations()?
        } else {
            document.structural_durations()?
        };
        let rate = document.presentation_basis().frame_rate;
        let mut nodes = BTreeMap::new();
        for (id, node) in document.nodes() {
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
                NodeKind::Sequence { children } => FrozenAudioKind::Sequence {
                    children: children.clone(),
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
            nodes.insert(
                id.clone(),
                FrozenAudioNode {
                    duration: durations[id],
                    edges: node.audio_edges,
                    editorial_edges: node.audio_editorial_edges,
                    kind,
                },
            );
        }
        Self::admit(LayoutWire {
            root: document.root().clone(),
            rate,
            nodes,
            overrides: document.overrides().clone(),
            gap_overrides: document.gap_overrides().clone(),
            audio_lineage: document.audio_lineage().clone(),
        })
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
                index: FrozenIndex::default(),
            }),
        };
        let index = layout.build_index()?;
        let mut data = Arc::into_inner(layout.inner).expect("a new layout has one owner");
        data.index = index;
        let layout = Self {
            inner: Arc::new(data),
        };
        // Capture and JSON admission share the serialized size ceiling.
        if check_size {
            layout.to_json()?;
        }
        Ok(layout)
    }

    /// Retain only the structure that projects the `required` aliases: each
    /// alias and its ancestors keep their exact nodes, override keys and
    /// lineage. Every other subtree becomes a duration-preserving spacer under
    /// its first alias, and runs of such Sequence siblings merge into one
    /// spacer (or vanish when their total is zero). Offsets, Repeat play
    /// layouts, Retime selections and ancestor crops are therefore unchanged
    /// for every retained alias, so its placements resolve identically.
    ///
    /// Only binding timing tables use this. Sound clocks compile and compare
    /// complete processing subtrees and must keep their full layout.
    pub(crate) fn sliced<'a>(
        &self,
        required: impl IntoIterator<Item = &'a NodeId>,
    ) -> Result<Self, DocumentError> {
        let mut keep = BTreeSet::new();
        for alias in required {
            if !self.nodes.contains_key(alias) {
                return Err(invalid("sliced frozen alias is missing"));
            }
            let mut current = alias;
            while keep.insert(current.clone()) {
                match self.index.parents.get(current) {
                    Some((parent, _)) => current = parent,
                    None => break,
                }
            }
        }
        let spacer = |duration: FrameDuration| FrozenAudioNode {
            duration,
            edges: AudioEdgePolicies::default(),
            editorial_edges: Default::default(),
            // A zero-duration leaf is invalid; an empty Sequence is the
            // zero-length structure with no audio of its own.
            kind: if duration == FrameDuration::ZERO {
                FrozenAudioKind::Sequence {
                    children: Vec::new(),
                }
            } else {
                FrozenAudioKind::Hold {
                    audio: ReferenceAudibility::Silence,
                }
            },
        };
        let mut nodes = BTreeMap::new();
        let mut pending = vec![&self.root];
        while let Some(id) = pending.pop() {
            let mut node = self.nodes[id].clone();
            match &mut node.kind {
                FrozenAudioKind::Sequence { children } => {
                    let mut retained = Vec::with_capacity(children.len());
                    let mut run: Option<(NodeId, FrameDuration)> = None;
                    let flush =
                        |run: &mut Option<(NodeId, FrameDuration)>,
                         retained: &mut Vec<NodeId>,
                         nodes: &mut BTreeMap<NodeId, FrozenAudioNode>| {
                            if let Some((first, duration)) = run.take()
                                && duration != FrameDuration::ZERO
                            {
                                nodes.insert(first.clone(), spacer(duration));
                                retained.push(first);
                            }
                        };
                    for child in self.nodes[id].kind.children() {
                        if keep.contains(child) {
                            flush(&mut run, &mut retained, &mut nodes);
                            retained.push(child.clone());
                            pending.push(child);
                        } else {
                            let duration = self.nodes[child].duration;
                            run = Some(match run.take() {
                                Some((first, total)) => (first, total.checked_add(duration)?),
                                None => (child.clone(), duration),
                            });
                        }
                    }
                    flush(&mut run, &mut retained, &mut nodes);
                    *children = retained;
                }
                FrozenAudioKind::Repeat { .. } | FrozenAudioKind::Retime { .. } => {
                    // Override and gap-branch roots keep their keys; only
                    // their contents collapse when nothing below is required.
                    for child in self.children(id) {
                        if keep.contains(child) {
                            pending.push(child);
                        } else {
                            nodes.insert(child.clone(), spacer(self.nodes[child].duration));
                        }
                    }
                }
                FrozenAudioKind::Source { .. } | FrozenAudioKind::Hold { .. } => {}
            }
            nodes.insert(id.clone(), node);
        }
        let retained_overrides = |overrides: &BTreeMap<NodeId, PlayOverrides>| {
            overrides
                .iter()
                .filter(|(repeat, _)| keep.contains(*repeat))
                .map(|(repeat, entries)| (repeat.clone(), entries.clone()))
                .collect()
        };
        Self::admit(LayoutWire {
            root: self.root.clone(),
            rate: self.rate,
            nodes,
            overrides: retained_overrides(&self.overrides),
            gap_overrides: retained_overrides(&self.gap_overrides),
            audio_lineage: self
                .audio_lineage
                .iter()
                .filter(|(id, _)| keep.contains(*id))
                .map(|(id, lineage)| (id.clone(), lineage.clone()))
                .collect(),
        })
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
        self.nodes[&self.root].duration
    }

    /// Prove that two complete processing subtrees correspond, allowing fresh
    /// node IDs while preserving exact clocks, Repeat identities, and controls.
    /// The scopes themselves cannot be project roots; only ordinary Sequence
    /// nodes may surround either scope.
    pub fn sound_clock_correspondence(
        &self,
        current: &Self,
        historical_scope: &NodeId,
        live_scope: &NodeId,
        maximum_work: usize,
    ) -> Result<SoundClockCorrespondence, DocumentError> {
        if maximum_work == 0 || maximum_work > MAX_DOCUMENT_NODES {
            return Err(limit("invalid sound clock comparison budget"));
        }
        if historical_scope == &self.root || live_scope == &current.root {
            return Err(invalid("root-owned sound clocks are not supported"));
        }
        if self.rate != current.rate {
            return Err(invalid("sound clock project rate changed"));
        }
        if !self.nodes.contains_key(historical_scope) || !current.nodes.contains_key(live_scope) {
            return Err(invalid("sound clock scope is missing"));
        }

        let mut work = 0usize;
        validate_sequence_scope(self, historical_scope, maximum_work, &mut work)?;
        validate_sequence_scope(current, live_scope, maximum_work, &mut work)?;

        let mut live_to_historical = BTreeMap::new();
        let mut historical_seen = BTreeSet::new();
        let mut live_parents = BTreeMap::new();
        let mut historical_parents = BTreeMap::new();
        let mut live_repeats = BTreeMap::new();
        let mut historical_repeats = BTreeMap::new();
        let mut pending = vec![(historical_scope.clone(), live_scope.clone())];
        while let Some((historical_id, live_id)) = pending.pop() {
            spend(&mut work, 1, maximum_work)?;
            if live_to_historical
                .insert(live_id.clone(), historical_id.clone())
                .is_some()
                || !historical_seen.insert(historical_id.clone())
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
                || !same_sound_clock_kind(&historical_node.kind, &live_node.kind)
            {
                return Err(invalid("sound clock processing subtree changed"));
            }

            let children = paired_sound_clock_children(self, current, &historical_id, &live_id)?;
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
            for (historical_child, live_child) in children {
                if historical_parents
                    .insert(historical_child.clone(), historical_id.clone())
                    .is_some()
                    || live_parents
                        .insert(live_child.clone(), live_id.clone())
                        .is_some()
                {
                    return Err(invalid("sound clock subtree contains a shared child"));
                }
                pending.push((historical_child, live_child));
            }
        }

        Ok(SoundClockCorrespondence {
            inner: Arc::new(SoundClockCorrespondenceInner {
                live_to_historical,
                historical_path: ScopedPathIndex {
                    scope: historical_scope.clone(),
                    parents: historical_parents,
                    repeats: historical_repeats,
                },
                live_path: ScopedPathIndex {
                    scope: live_scope.clone(),
                    parents: live_parents,
                    repeats: live_repeats,
                },
                work,
            }),
        })
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
        let mut seen = BTreeSet::new();
        let mut stack = vec![(&self.root, 0usize, false)];
        let mut durations = BTreeMap::new();
        let mut index = FrozenIndex::default();
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
                    let repeat = RepeatLayout::compile_with_gap_overrides(
                        iterations,
                        child,
                        self.overrides.get(id),
                        *gap_duration,
                        self.gap_overrides.get(id),
                        &durations,
                    )?;
                    let duration = repeat.duration();
                    index.repeats.insert(id.clone(), repeat);
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
            durations.insert(id.clone(), computed);
            let mut offset = 0i64;
            for child in self.children(id) {
                index.parents.insert(child.clone(), (id.clone(), offset));
                if matches!(node.kind, FrozenAudioKind::Sequence { .. }) {
                    offset = offset
                        .checked_add(durations[child].frames())
                        .ok_or_else(|| invalid("frozen sequence offset overflow"))?;
                }
            }
        }
        if seen.len() != self.nodes.len() {
            return Err(invalid("frozen audio contains unreachable nodes"));
        }
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

    pub(crate) fn project_scoped_supported(
        &self,
        root: &NodeId,
        instance: &InstancePath,
        maximum_work: usize,
    ) -> Result<(FrozenAudioProjection, std::ops::Range<ExactRatio>), DocumentError> {
        self.project_scoped_with_support(root, instance, None, maximum_work)
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
        if maximum_work == 0 || maximum_work > MAX_DOCUMENT_NODES {
            return Err(limit("invalid frozen projection budget"));
        }
        instance.validate_depth()?;
        if !self.nodes.contains_key(root) {
            return Err(invalid("frozen projection scope is missing"));
        }
        let target = self
            .nodes
            .get(&instance.node)
            .ok_or_else(|| invalid("frozen projection host is missing"))?;
        let mut work = 0usize;
        let mut origin = ExactRatio::ZERO;
        let mut scale = ExactRatio::ONE;
        let mut local_duration = target.duration;
        if let Some(gap) = gap_after {
            let layout = self
                .index
                .repeats
                .get(&instance.node)
                .ok_or_else(|| invalid("frozen gap scope is not a Repeat"))?;
            spend(&mut work, layout.segment_count(), maximum_work)?;
            let play = layout
                .play(gap)
                .ok_or_else(|| invalid("frozen gap play is missing"))?;
            if play.gap_after == FrameDuration::ZERO || play.gap_child.is_some() {
                return Err(invalid("frozen play has no following gap"));
            }
            origin = ExactRatio::integer(play.start)
                .checked_add(ExactRatio::integer(play.duration.frames()))?;
            local_duration = play.gap_after;
        }
        let mut support = ExactRatio::ZERO..ExactRatio::integer(local_duration.frames());
        let mut node = &instance.node;
        let mut step = instance.repeats.len();
        loop {
            spend(&mut work, 1, maximum_work)?;
            if node == root {
                break;
            }
            let (parent, offset) = self
                .index
                .parents
                .get(node)
                .ok_or_else(|| invalid("frozen projection host is outside scope"))?;
            match &self.nodes[parent].kind {
                FrozenAudioKind::Sequence { .. } => {
                    origin = origin.checked_add(ExactRatio::integer(*offset))?
                }
                FrozenAudioKind::Retime {
                    mapping,
                    pitch,
                    purpose,
                    ..
                } => {
                    if mode != ProjectionMode::Affine
                        && *pitch == PitchPolicy::Preserve
                        && mapping.duration() != self.nodes[parent].duration
                    {
                        return Err(invalid("frozen support scope crosses an opaque Preserve"));
                    }
                    if mode == ProjectionMode::VisibleAllocation
                        || (mode == ProjectionMode::MeaningfulSupport
                            && *purpose != RetimePurpose::Partition)
                    {
                        let selected = ExactRatio::integer(mapping.start().0)
                            .checked_sub(origin)?
                            .checked_div(scale)?
                            ..ExactRatio::integer(mapping.end().0)
                                .checked_sub(origin)?
                                .checked_div(scale)?;
                        clip_binding_support(&mut support, selected)?;
                    }
                    let factor = ExactRatio::new(
                        i128::from(self.nodes[parent].duration.frames()),
                        i128::from(mapping.duration().frames()),
                    )?;
                    origin = origin
                        .checked_sub(ExactRatio::integer(mapping.start().0))?
                        .checked_mul(factor)?;
                    scale = scale.checked_mul(factor)?;
                }
                FrozenAudioKind::Repeat { .. } => {
                    step = step
                        .checked_sub(1)
                        .ok_or_else(|| invalid("frozen occurrence omits a Repeat"))?;
                    let selected = &instance.repeats[step];
                    if &selected.node != parent {
                        return Err(invalid("frozen Repeat path order is wrong"));
                    }
                    let layout = &self.index.repeats[parent];
                    spend(&mut work, layout.segment_count(), maximum_work)?;
                    let play = layout
                        .play(&selected.iteration)
                        .ok_or_else(|| invalid("frozen occurrence play is missing"))?;
                    let offset = if &play.child == node {
                        play.start
                    } else if play.gap_child.as_ref() == Some(node) {
                        // A retained dormant branch still has an affine clock
                        // and meaningful recipe; it contributes no allocation.
                        if mode == ProjectionMode::VisibleAllocation
                            && play.gap_after == FrameDuration::ZERO
                        {
                            support.end = support.start;
                        }
                        play.start
                            .checked_add(play.duration.frames())
                            .ok_or(TimeError::Overflow)?
                    } else {
                        return Err(invalid(
                            "frozen occurrence selects the wrong override child",
                        ));
                    };
                    origin = origin.checked_add(ExactRatio::integer(offset))?;
                }
                _ => return Err(invalid("frozen projection has a leaf parent")),
            }
            node = parent;
        }
        if step != 0 {
            return Err(invalid("frozen occurrence has extra Repeat ancestors"));
        }
        Ok((
            FrozenAudioProjection {
                origin,
                frames_per_local_frame: scale,
                point: origin.checked_add(local.checked_mul(scale)?)?,
                local_duration,
                instance: instance.clone(),
                gap_after: gap_after.cloned(),
                work,
            },
            support,
        ))
    }

    /// Structural Repeat ancestors from outside inward, without selecting any
    /// play. This also reaches an unplayed default child and sparse overrides.
    /// The path stays within one physical clock, including its explicit root.
    pub(crate) fn scoped_repeats(
        &self,
        root: &NodeId,
        target: &NodeId,
        maximum_work: usize,
    ) -> Result<(Vec<NodeId>, usize), DocumentError> {
        if maximum_work == 0 || maximum_work > MAX_DOCUMENT_NODES {
            return Err(limit("invalid frozen scope budget"));
        }
        if !self.nodes.contains_key(root) || !self.nodes.contains_key(target) {
            return Err(invalid("frozen scope alias is missing"));
        }
        let mut node = target;
        let mut repeats = Vec::new();
        let mut work = 0;
        loop {
            spend(&mut work, 1, maximum_work)?;
            if node == root {
                break;
            }
            let (parent, _) = self
                .index
                .parents
                .get(node)
                .ok_or_else(|| invalid("frozen scope does not contain its target"))?;
            let parent_node = &self.nodes[parent];
            if matches!(
                &parent_node.kind,
                FrozenAudioKind::Retime {
                    mapping,
                    pitch: PitchPolicy::Preserve,
                    ..
                } if mapping.duration() != parent_node.duration
            ) {
                return Err(invalid("audio binding scope crosses an opaque Preserve"));
            }
            if matches!(self.nodes[parent].kind, FrozenAudioKind::Repeat { .. }) {
                repeats.push(parent.clone());
            }
            node = parent;
        }
        repeats.reverse();
        Ok((repeats, work))
    }

    /// Immediate owned child on the target's path below an explicit ancestor.
    /// Sparse overrides are indexed parents, so unrelated branches cost no work.
    pub(crate) fn branch_below(
        &self,
        ancestor: &NodeId,
        target: &NodeId,
        maximum_work: usize,
    ) -> Result<(NodeId, usize), DocumentError> {
        if maximum_work == 0 || maximum_work > MAX_DOCUMENT_NODES {
            return Err(limit("invalid frozen branch budget"));
        }
        if !self.nodes.contains_key(ancestor) || !self.nodes.contains_key(target) {
            return Err(invalid("frozen branch alias is missing"));
        }
        let mut node = target;
        let mut work = 0;
        loop {
            spend(&mut work, 1, maximum_work)?;
            let (parent, _) = self
                .index
                .parents
                .get(node)
                .ok_or_else(|| invalid("frozen ancestor does not contain its target"))?;
            if parent == ancestor {
                return Ok((node.clone(), work));
            }
            node = parent;
        }
    }
}

fn validate_sequence_scope(
    layout: &FrozenAudioLayout,
    scope: &NodeId,
    maximum_work: usize,
    work: &mut usize,
) -> Result<(), DocumentError> {
    let mut node = scope;
    while node != &layout.root {
        let (parent, _) = layout
            .index
            .parents
            .get(node)
            .ok_or_else(|| invalid("sound clock scope is outside its project root"))?;
        spend(work, 1, maximum_work)?;
        if !matches!(layout.nodes[parent].kind, FrozenAudioKind::Sequence { .. }) {
            return Err(invalid(
                "sound clock scope may only be surrounded by ordinary Sequences",
            ));
        }
        node = parent;
    }
    Ok(())
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
    if maximum_work == 0 || maximum_work > MAX_DOCUMENT_NODES {
        return Err(limit("invalid sound clock path budget"));
    }
    instance.validate_depth()?;
    let mut node = &instance.node;
    let mut repeat_step = instance.repeats.len();
    let mut work = 0usize;
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
            if repeat.iterations.position(&selected.iteration).is_none() {
                return Err(invalid("sound clock occurrence names a retired iteration"));
            }
            let child = repeat
                .overrides
                .get(&selected.iteration)
                .unwrap_or(&repeat.child);
            let gap_child = repeat.gap_overrides.get(&selected.iteration);
            if child != node && gap_child != Some(node) {
                return Err(invalid(
                    "sound clock occurrence selects the wrong override branch",
                ));
            }
        }
        node = parent;
    }
    if repeat_step != 0 {
        return Err(invalid("sound clock occurrence has extra Repeat ancestors"));
    }
    Ok(work)
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
