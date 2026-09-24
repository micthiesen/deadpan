//! Immutable timing and audibility facts. Aliases belong to this layout, never
//! to a later live document; no media, marks or previous bindings are retained.

mod preflight;

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

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
    Tail { maximum: FrameDuration },
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum AudibilityWire {
    Silence {},
    RoomTone {},
    Tail { maximum: FrameDuration },
}

impl<'de> Deserialize<'de> for ReferenceAudibility {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let audio = match AudibilityWire::deserialize(deserializer)? {
            AudibilityWire::Silence {} => Self::Silence,
            AudibilityWire::RoomTone {} => Self::RoomTone,
            AudibilityWire::Tail { maximum } => Self::Tail { maximum },
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
                maximum: FrameDuration::ZERO
            }
        ) {
            return Err(invalid("frozen Tail maximum must be positive"));
        }
        Ok(())
    }

    fn validate_duration(self, duration: FrameDuration) -> Result<(), DocumentError> {
        self.validate()?;
        if let Self::Tail { maximum } = self
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
            HoldAudio::Tail { maximum, .. } => Self::Tail { maximum: *maximum },
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
    pub kind: FrozenAudioKind,
}

#[derive(Debug, Clone, Default)]
struct FrozenIndex {
    parents: BTreeMap<NodeId, (NodeId, i64)>,
    repeats: BTreeMap<NodeId, RepeatLayout>,
}

/// Admitted through capture or bounded JSON only. Cached structural indexes
/// make projection independent of layout size and never expand Repeat plays.
#[derive(Debug, Clone, Serialize)]
pub struct FrozenAudioLayout {
    root: NodeId,
    rate: FrameRate,
    nodes: BTreeMap<NodeId, FrozenAudioNode>,
    overrides: BTreeMap<NodeId, PlayOverrides>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    audio_lineage: BTreeMap<NodeId, AudioLineageId>,
    #[serde(skip)]
    index: FrozenIndex,
}

impl PartialEq for FrozenAudioLayout {
    fn eq(&self, other: &Self) -> bool {
        self.root == other.root
            && self.rate == other.rate
            && self.nodes == other.nodes
            && self.overrides == other.overrides
            && self.audio_lineage == other.audio_lineage
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

impl FrozenAudioLayout {
    pub fn capture(document: &ProjectDocument) -> Result<Self, DocumentError> {
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
        let durations = document.durations()?;
        let rate = document.presentation_basis().frame_rate;
        let mut nodes = BTreeMap::new();
        for (id, node) in document.nodes() {
            let kind = match &node.kind {
                NodeKind::Source { source } => {
                    let placement = source
                        .audio
                        .as_ref()
                        .map(|_| {
                            let start = source
                                .audio_mapping
                                .start_frames_with_offset(source.audio_offset, rate)?;
                            let end = start.checked_add(
                                source.audio_mapping.duration_frames(source.duration)?,
                            )?;
                            ExactFrameRange::new(start, end)
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
                NodeKind::Repeat {
                    child,
                    iterations,
                    gap,
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
                    kind,
                },
            );
        }
        Self::admit(LayoutWire {
            root: document.root().clone(),
            rate,
            nodes,
            overrides: document.overrides().clone(),
            audio_lineage: document.audio_lineage().clone(),
        })
    }

    fn admit(wire: LayoutWire) -> Result<Self, DocumentError> {
        let mut layout = Self {
            root: wire.root,
            rate: wire.rate,
            nodes: wire.nodes,
            overrides: wire.overrides,
            audio_lineage: wire.audio_lineage,
            index: FrozenIndex::default(),
        };
        layout.index = layout.build_index()?;
        // Capture and JSON admission share the serialized size ceiling.
        layout.to_json()?;
        Ok(layout)
    }

    pub fn from_json(json: &str) -> Result<Self, DocumentError> {
        if json.len() > MAX_DOCUMENT_JSON_BYTES {
            return Err(limit("frozen audio JSON exceeds byte limit"));
        }
        preflight::check(json)?;
        Self::admit(serde_json::from_str(json).map_err(DocumentError::json)?)
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
    /// Authored copy provenance keyed by owned frozen aliases. Token origins
    /// are historical names, never live references or admission to media/PCM.
    pub fn audio_lineage(&self) -> &BTreeMap<NodeId, AudioLineageId> {
        &self.audio_lineage
    }
    pub fn duration(&self) -> FrameDuration {
        self.nodes[&self.root].duration
    }

    fn children<'a>(&'a self, id: &NodeId) -> impl DoubleEndedIterator<Item = &'a NodeId> {
        self.nodes[id].kind.children().iter().chain(
            self.overrides
                .get(id)
                .into_iter()
                .flat_map(|entries| entries.iter().map(|(_, child)| child)),
        )
    }

    fn build_index(&self) -> Result<FrozenIndex, DocumentError> {
        if self.nodes.len() > MAX_DOCUMENT_NODES
            || self.overrides.len() > MAX_DOCUMENT_NODES
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
        for (owner, entries) in &self.overrides {
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
                    let repeat = RepeatLayout::compile(
                        iterations,
                        child,
                        self.overrides.get(id),
                        *gap_duration,
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
        self.project_scoped_inner(root, instance, local, gap_after, maximum_work, false)
            .map(|(projection, _)| projection)
    }

    pub(crate) fn project_scoped_supported(
        &self,
        root: &NodeId,
        instance: &InstancePath,
        maximum_work: usize,
    ) -> Result<(FrozenAudioProjection, std::ops::Range<ExactRatio>), DocumentError> {
        self.project_scoped_inner(root, instance, ExactRatio::ZERO, None, maximum_work, true)
    }

    fn project_scoped_inner(
        &self,
        root: &NodeId,
        instance: &InstancePath,
        local: ExactRatio,
        gap_after: Option<&IterationId>,
        maximum_work: usize,
        capture_support: bool,
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
        let mut support = ExactRatio::ZERO..ExactRatio::integer(local_duration.frames());
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
            if play.gap_after == FrameDuration::ZERO {
                return Err(invalid("frozen play has no following gap"));
            }
            origin = ExactRatio::integer(play.start)
                .checked_add(ExactRatio::integer(play.duration.frames()))?;
            local_duration = play.gap_after;
        }
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
                    mapping, purpose, ..
                } => {
                    if capture_support && *purpose != crate::RetimePurpose::Partition {
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
                    if &play.child != node {
                        return Err(invalid(
                            "frozen occurrence selects the wrong override child",
                        ));
                    }
                    origin = origin.checked_add(ExactRatio::integer(play.start))?;
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
        ExactFrameRange::new(placement.start, placement.end)?;
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
        && (node.duration != mapping.duration() || !node.edges.is_automatic())
    {
        return Err(invalid(
            "frozen Partition requires unity timing and automatic edges",
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
