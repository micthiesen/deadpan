//! Owned raw recipes retain sampling clocks, not historical media bodies.
//! Timing records are flat and bounded. Local phase expressions may reference
//! several records; no record contains bindings or references another record.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io::{self, Write};

use serde::de::{self, DeserializeSeed, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde::ser::{SerializeSeq, SerializeStruct};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::value::RawValue;

use crate::{
    DocumentError, DocumentErrorCode, ExactFrameRange, ExactRatio, FrameDuration, FrozenAudioKind,
    FrozenAudioLayout, InstancePath, IterationId, MAX_DOCUMENT_DEPTH, MAX_DOCUMENT_JSON_BYTES,
    MAX_DOCUMENT_NODES, MIX_SAMPLE_RATE, NodeId, NodeKind, PitchPolicy, ProjectDocument,
    RepeatInstance, RevisionId, TimeError,
};

pub const MAX_AUDIO_BINDING_TERMS: usize = 256;
pub const MAX_AUDIO_BINDING_ENTRIES: usize = 100_000;
const MAX_BINDING_WIRE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioTimingId {
    pub allocation: RevisionId,
    pub ordinal: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum AudioClockRoot {
    ProjectRootRoundEven,
    PreserveInputPointCeil { stage: NodeId },
    DefinitionPointCeil { root: NodeId },
    GapDefinitionPointCeil { repeat: NodeId },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioRecipeKind {
    #[default]
    Node,
    RepeatGap,
}

impl AudioRecipeKind {
    fn is_node(&self) -> bool {
        *self == Self::Node
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioReferenceClock {
    pub timing: AudioTimingId,
    pub root: AudioClockRoot,
    pub physical: NodeId,
    #[serde(default, skip_serializing_if = "AudioRecipeKind::is_node")]
    pub recipe: AudioRecipeKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum AudioRepeatValue {
    Live { repeat: NodeId },
    Captured { iteration: IterationId },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioRepeatArgument {
    pub reference_repeat: NodeId,
    pub value: AudioRepeatValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum AudioBirthSurvivors {
    CapturedRepeat {
        repeat: NodeId,
    },
    Run {
        allocation: RevisionId,
        first: u32,
        count: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioBirthClause {
    pub repeat: NodeId,
    pub survivors: AudioBirthSurvivors,
    pub definition_root: NodeId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioPlacementTemplate {
    pub reference: AudioReferenceClock,
    /// Historical physical-local coordinate = current physical-local coordinate
    /// + this offset. It translates the origin, never the rate or sample grid.
    #[serde(
        default = "zero_reference_local_offset",
        skip_serializing_if = "is_zero_reference_local_offset"
    )]
    pub reference_local_offset: ExactRatio,
    /// The gap's own preceding play is separate from its outer Repeat path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gap_after: Option<AudioRepeatValue>,
    #[serde(deserialize_with = "path_vec")]
    pub arguments: Vec<AudioRepeatArgument>,
    /// Outer-to-inner lexical order. The innermost matching birth wins.
    #[serde(deserialize_with = "path_vec")]
    pub births: Vec<AudioBirthClause>,
}

fn zero_reference_local_offset() -> ExactRatio {
    ExactRatio::ZERO
}
fn is_zero_reference_local_offset(value: &ExactRatio) -> bool {
    *value == ExactRatio::ZERO
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioPhaseTerm {
    pub placement: AudioPlacementTemplate,
    pub from_local: ExactRatio,
    pub to_local: ExactRatio,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioLocalPhase {
    pub constant: ExactRatio,
    #[serde(deserialize_with = "term_vec")]
    pub terms: Vec<AudioPhaseTerm>,
}

impl Default for AudioLocalPhase {
    fn default() -> Self {
        Self {
            constant: ExactRatio::ZERO,
            terms: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioResume {
    pub local_boundary: ExactRatio,
    pub phase: AudioLocalPhase,
}

/// A closed structural boundary, independent of Source audio support.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioSourceEndpoint {
    Start,
    End,
}

/// Select an entry on each occurrence's captured clock without baking its phase.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum AudioReanchorAnchor {
    #[default]
    AllocationEntry,
    SourceEndpoint {
        endpoint: AudioSourceEndpoint,
    },
}

impl<'de> Deserialize<'de> for AudioReanchorAnchor {
    fn deserialize<D: Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        // Tagged unit variants otherwise ignore extra fields despite the enum's
        // deny_unknown_fields. Struct variants keep both choices closed.
        #[derive(Deserialize)]
        #[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
        enum Wire {
            AllocationEntry {},
            SourceEndpoint { endpoint: AudioSourceEndpoint },
        }
        Ok(match Wire::deserialize(decoder)? {
            Wire::AllocationEntry {} => Self::AllocationEntry,
            Wire::SourceEndpoint { endpoint } => Self::SourceEndpoint { endpoint },
        })
    }
}

impl AudioReanchorAnchor {
    pub fn is_allocation_entry(&self) -> bool {
        matches!(self, Self::AllocationEntry)
    }
}

/// A chronological reanchor on a retained allocation, evaluated separately for
/// each effective occurrence. The window uses the placement's captured scope;
/// an inner definition birth drops an enclosing window, not intrinsic crops.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioReanchorStep {
    pub placement: AudioPlacementTemplate,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<ExactFrameRange>,
    #[serde(
        default,
        skip_serializing_if = "AudioReanchorAnchor::is_allocation_entry"
    )]
    pub anchor: AudioReanchorAnchor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnedAudioBinding {
    pub lattice: AudioPlacementTemplate,
    pub resume: Option<AudioResume>,
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "term_vec"
    )]
    pub reanchors: Vec<AudioReanchorStep>,
}

impl OwnedAudioBinding {
    /// Re-express this binding after a signed translation of its physical local
    /// coordinates: `new_local = old_local + prefix`. A positive prefix moves
    /// existing material later in the new physical domain. Negative values undo
    /// that translation; the caller separately owns current recipe validation.
    ///
    /// Historical layouts and enclosing reanchor windows keep their clocks.
    /// Phase terms keep their exact sample distances. This pure operation changes
    /// neither the binding nor a document when checked arithmetic fails.
    pub fn rebase_local(&self, prefix: ExactRatio) -> Result<Self, DocumentError> {
        let terms = self
            .resume
            .as_ref()
            .map_or(0, |resume| resume.phase.terms.len());
        if terms
            .checked_add(self.reanchors.len())
            .is_none_or(|count| count > MAX_AUDIO_BINDING_TERMS)
        {
            return Err(limit("audio phase term and reanchor count"));
        }
        binding_wire_size(self)?;
        let mut result = self.clone();
        for placement in result.placements_mut() {
            placement.reference_local_offset =
                placement.reference_local_offset.checked_sub(prefix)?;
        }
        if let Some(resume) = &mut result.resume {
            resume.local_boundary = resume.local_boundary.checked_add(prefix)?;
            for term in &mut resume.phase.terms {
                term.from_local = term.from_local.checked_add(prefix)?;
                term.to_local = term.to_local.checked_add(prefix)?;
            }
        }
        binding_wire_size(&result)?;
        Ok(result)
    }

    pub(crate) fn placements(&self) -> impl Iterator<Item = &AudioPlacementTemplate> {
        std::iter::once(&self.lattice)
            .chain(
                self.resume
                    .iter()
                    .flat_map(|resume| resume.phase.terms.iter().map(|term| &term.placement)),
            )
            .chain(self.reanchors.iter().map(|step| &step.placement))
    }

    pub(crate) fn placements_mut(&mut self) -> impl Iterator<Item = &mut AudioPlacementTemplate> {
        std::iter::once(&mut self.lattice)
            .chain(self.resume.iter_mut().flat_map(|resume| {
                resume
                    .phase
                    .terms
                    .iter_mut()
                    .map(|term| &mut term.placement)
            }))
            .chain(self.reanchors.iter_mut().map(|step| &mut step.placement))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AudioTimingRecord {
    pub id: AudioTimingId,
    pub layout: FrozenAudioLayout,
}

/// Closed serializable intent. Source/recipe admission belongs to the current
/// owned document and the media host; these records only establish coordinates.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AudioBindingState {
    pub(crate) timings: BTreeMap<AudioTimingId, FrozenAudioLayout>,
    pub(crate) bindings: BTreeMap<NodeId, OwnedAudioBinding>,
    pub(crate) gap_bindings: BTreeMap<NodeId, OwnedAudioBinding>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioDefinitionScope<'a> {
    NodeOutput(&'a NodeId),
    RepeatGap(&'a NodeId),
}

impl<'a> AudioDefinitionScope<'a> {
    fn node(self) -> &'a NodeId {
        match self {
            Self::NodeOutput(node) | Self::RepeatGap(node) => node,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum AudioBindingEnvironment<'a> {
    Occurrence(&'a InstancePath),
    /// The caller obtains outside_repeats from the current definition root's
    /// actual ancestry. Only these explicitly excluded arguments become births.
    Definition {
        root: &'a NodeId,
        instance: &'a InstancePath,
        outside_repeats: &'a [NodeId],
    },
    GapOccurrence {
        instance: &'a InstancePath,
        after: &'a IterationId,
    },
    GapDefinition {
        root: AudioDefinitionScope<'a>,
        instance: &'a InstancePath,
        outside_repeats: &'a [NodeId],
        after: Option<&'a IterationId>,
    },
}

impl AudioBindingEnvironment<'_> {
    fn lookup_work(&self) -> usize {
        self.instance().repeats.len()
            + match self {
                Self::Occurrence(_) | Self::GapOccurrence { .. } => 0,
                Self::Definition {
                    outside_repeats, ..
                }
                | Self::GapDefinition {
                    outside_repeats, ..
                } => outside_repeats.len(),
            }
    }
    fn instance(&self) -> &InstancePath {
        match self {
            Self::Occurrence(instance)
            | Self::Definition { instance, .. }
            | Self::GapOccurrence { instance, .. }
            | Self::GapDefinition { instance, .. } => instance,
        }
    }
    fn excluded(&self, repeat: &NodeId) -> bool {
        matches!(self, Self::Definition { outside_repeats, .. } | Self::GapDefinition { outside_repeats, .. } if outside_repeats.contains(repeat))
    }
    fn validate(&self) -> Result<(), DocumentError> {
        self.instance().validate_depth()?;
        let gap = matches!(
            self,
            Self::GapOccurrence { .. } | Self::GapDefinition { .. }
        );
        let mut seen = BTreeSet::new();
        for repeat in &self.instance().repeats {
            if gap && repeat.node == self.instance().node {
                return Err(invalid(
                    "own gap argument belongs outside the outer Repeat path",
                ));
            }
            if !seen.insert(&repeat.node) {
                return Err(invalid("duplicate live Repeat argument"));
            }
        }
        if let Self::Definition {
            outside_repeats, ..
        }
        | Self::GapDefinition {
            outside_repeats, ..
        } = self
        {
            if outside_repeats.len() > MAX_DOCUMENT_DEPTH {
                return Err(limit("definition exclusion depth"));
            }
            for repeat in *outside_repeats {
                if gap && repeat == &self.instance().node {
                    return Err(invalid("gap owner cannot be an excluded outer Repeat"));
                }
                if !seen.insert(repeat) {
                    return Err(invalid("duplicate or present excluded Repeat"));
                }
            }
        }
        Ok(())
    }
    fn gap_after(&self) -> Option<&IterationId> {
        match self {
            Self::GapOccurrence { after, .. } => Some(after),
            Self::GapDefinition { after, .. } => *after,
            _ => None,
        }
    }
    fn gap_definition(&self) -> bool {
        matches!(self, Self::GapDefinition { root: AudioDefinitionScope::RepeatGap(root), instance, after: None, .. } if *root == &instance.node)
    }
    fn iteration(&self, repeat: &NodeId) -> Option<&IterationId> {
        self.instance()
            .repeats
            .iter()
            .find(|value| &value.node == repeat)
            .map(|value| &value.iteration)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioBindingGridRule {
    RootRoundEven,
    PointCeil,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedAudioPlacement {
    pub clock: AudioClockRoot,
    pub grid_rule: AudioBindingGridRule,
    pub grid_origin: ExactRatio,
    pub frames_per_sample: ExactRatio,
    pub origin: ExactRatio,
    pub frames_per_local_frame: ExactRatio,
    /// Historical recipe duration; translation does not make its domain start at zero.
    pub local_duration: FrameDuration,
    /// Meaningful captured Edit/clock support in current physical-local frames. Source
    /// placement and audibility still come from the current owned recipe.
    pub local_support: std::ops::Range<ExactRatio>,
    pub instance: InstancePath,
    pub gap_after: Option<IterationId>,
    pub birth: Option<usize>,
    pub gap_birth: bool,
    pub work: usize,
}

impl ResolvedAudioPlacement {
    pub fn local_frames_per_sample(&self) -> Result<ExactRatio, TimeError> {
        self.frames_per_sample
            .checked_div(self.frames_per_local_frame)
    }
    /// Clock indices remain i64 just like the consuming root/point readers.
    pub fn sample_boundary(&self, local: ExactRatio) -> Result<i64, TimeError> {
        let frame = self
            .origin
            .checked_add(local.checked_mul(self.frames_per_local_frame)?)?;
        let value = frame
            .checked_sub(self.grid_origin)?
            .checked_div(self.frames_per_sample)?;
        i64::try_from(match self.grid_rule {
            AudioBindingGridRule::RootRoundEven => value.round_even()?,
            AudioBindingGridRule::PointCeil => value.ceil()?,
        })
        .map_err(|_| TimeError::Overflow)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedAudioResume {
    pub local_boundary: ExactRatio,
    pub reference_local_delta: ExactRatio,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedAudioBinding {
    pub lattice: ResolvedAudioPlacement,
    pub resume: Option<ResolvedAudioResume>,
    pub work: usize,
}

struct Work {
    used: usize,
    maximum: usize,
}
impl Work {
    fn new(maximum: usize) -> Result<Self, DocumentError> {
        if maximum == 0 || maximum > MAX_DOCUMENT_NODES {
            return Err(limit("invalid audio binding work limit"));
        }
        Ok(Self { used: 0, maximum })
    }
    fn spend(&mut self, count: usize) -> Result<(), DocumentError> {
        self.used = self
            .used
            .checked_add(count)
            .ok_or_else(|| limit("audio binding work overflow"))?;
        if self.used > self.maximum {
            return Err(limit("audio binding work exhausted"));
        }
        Ok(())
    }
    fn remaining(&self) -> Result<usize, DocumentError> {
        self.maximum
            .checked_sub(self.used)
            .filter(|count| *count > 0)
            .ok_or_else(|| limit("audio binding work exhausted"))
    }
}

struct ClockScope<'a> {
    root: AudioDefinitionScope<'a>,
    grid_origin: ExactRatio,
    rule: AudioBindingGridRule,
    support: Option<std::ops::Range<ExactRatio>>,
}

fn clock_scope<'a>(
    layout: &'a FrozenAudioLayout,
    clock: &'a AudioClockRoot,
) -> Result<ClockScope<'a>, DocumentError> {
    match clock {
        AudioClockRoot::ProjectRootRoundEven => Ok(ClockScope {
            root: AudioDefinitionScope::NodeOutput(layout.root()),
            grid_origin: ExactRatio::ZERO,
            rule: AudioBindingGridRule::RootRoundEven,
            support: None,
        }),
        AudioClockRoot::DefinitionPointCeil { root } => {
            if !layout.nodes().contains_key(root) {
                return Err(invalid("definition clock root is missing"));
            }
            Ok(ClockScope {
                root: AudioDefinitionScope::NodeOutput(root),
                grid_origin: ExactRatio::ZERO,
                rule: AudioBindingGridRule::PointCeil,
                support: None,
            })
        }
        AudioClockRoot::PreserveInputPointCeil { stage } => {
            let node = layout
                .nodes()
                .get(stage)
                .ok_or_else(|| invalid("Preserve clock owner is missing"))?;
            let FrozenAudioKind::Retime {
                child,
                mapping,
                pitch: PitchPolicy::Preserve,
                ..
            } = &node.kind
            else {
                return Err(invalid("point clock owner is not Preserve"));
            };
            if mapping.duration() == node.duration {
                return Err(invalid("unity Retime has no input preparation clock"));
            }
            Ok(ClockScope {
                root: AudioDefinitionScope::NodeOutput(child),
                grid_origin: ExactRatio::integer(mapping.start().0),
                rule: AudioBindingGridRule::PointCeil,
                support: Some(
                    ExactRatio::integer(mapping.start().0)..ExactRatio::integer(mapping.end().0),
                ),
            })
        }
        AudioClockRoot::GapDefinitionPointCeil { repeat } => {
            gap_duration(layout, repeat)?;
            Ok(ClockScope {
                root: AudioDefinitionScope::RepeatGap(repeat),
                grid_origin: ExactRatio::ZERO,
                rule: AudioBindingGridRule::PointCeil,
                support: None,
            })
        }
    }
}

fn gap_duration(
    layout: &FrozenAudioLayout,
    repeat: &NodeId,
) -> Result<FrameDuration, DocumentError> {
    match layout.nodes().get(repeat).map(|node| &node.kind) {
        Some(FrozenAudioKind::Repeat { gap_duration, .. })
            if *gap_duration != FrameDuration::ZERO =>
        {
            Ok(*gap_duration)
        }
        _ => Err(invalid(
            "audio gap binding requires a configured positive gap",
        )),
    }
}

impl ClockScope<'_> {
    fn project(
        &self,
        layout: &FrozenAudioLayout,
        instance: &InstancePath,
        gap_after: Option<&IterationId>,
        allocation: bool,
        work: &mut Work,
    ) -> Result<
        (
            crate::FrozenAudioProjection,
            Option<std::ops::Range<ExactRatio>>,
        ),
        DocumentError,
    > {
        if let AudioDefinitionScope::RepeatGap(repeat) = self.root {
            if repeat != &instance.node || !instance.repeats.is_empty() || gap_after.is_some() {
                return Err(invalid("gap definition has occurrence arguments"));
            }
            work.spend(1)?;
            let duration = gap_duration(layout, repeat)?;
            return Ok((
                crate::FrozenAudioProjection {
                    origin: ExactRatio::ZERO,
                    frames_per_local_frame: ExactRatio::ONE,
                    point: ExactRatio::ZERO,
                    local_duration: duration,
                    instance: instance.clone(),
                    gap_after: None,
                    work: 0,
                },
                Some(ExactRatio::ZERO..ExactRatio::integer(duration.frames())),
            ));
        }
        let result = if allocation {
            layout.project_scoped_with_allocation(
                self.root.node(),
                instance,
                gap_after,
                work.remaining()?,
            )?
        } else {
            let (projection, support) = match gap_after {
                Some(after) => layout.project_scoped_with_support(
                    self.root.node(),
                    instance,
                    Some(after),
                    work.remaining()?,
                )?,
                None => layout.project_scoped_supported(
                    self.root.node(),
                    instance,
                    work.remaining()?,
                )?,
            };
            (projection, Some(support))
        };
        work.spend(result.0.work)?;
        Ok(result)
    }
}

fn physical(kind: &FrozenAudioKind, duration: FrameDuration) -> bool {
    matches!(
        kind,
        FrozenAudioKind::Source { .. } | FrozenAudioKind::Hold { .. }
    ) || matches!(kind, FrozenAudioKind::Retime { mapping, pitch: PitchPolicy::Preserve, .. } if mapping.duration() != duration)
}

impl AudioPlacementTemplate {
    pub fn validate(&self, layout: &FrozenAudioLayout) -> Result<(), DocumentError> {
        let mut work = Work::new(MAX_DOCUMENT_NODES)?;
        self.validate_with(layout, &mut work)
    }

    fn validate_with(
        &self,
        layout: &FrozenAudioLayout,
        work: &mut Work,
    ) -> Result<(), DocumentError> {
        if self.arguments.len() > MAX_DOCUMENT_DEPTH || self.births.len() > MAX_DOCUMENT_DEPTH {
            return Err(limit("audio binding lexical depth"));
        }
        work.spend(self.entry_count())?;
        let scope = clock_scope(layout, &self.reference.root)?;
        let target = layout
            .nodes()
            .get(&self.reference.physical)
            .ok_or_else(|| invalid("audio binding physical alias is missing"))?;
        match self.reference.recipe {
            AudioRecipeKind::Node => {
                if !physical(&target.kind, target.duration)
                    || self.gap_after.is_some()
                    || matches!(scope.root, AudioDefinitionScope::RepeatGap(_))
                {
                    return Err(invalid("audio binding requires a node physical recipe"));
                }
            }
            AudioRecipeKind::RepeatGap => {
                gap_duration(layout, &self.reference.physical)?;
                if let AudioDefinitionScope::RepeatGap(repeat) = scope.root {
                    if repeat != &self.reference.physical
                        || self.gap_after.is_some()
                        || !self.arguments.is_empty()
                        || !self.births.is_empty()
                    {
                        return Err(invalid("gap definition arguments are not canonical"));
                    }
                } else if self.gap_after.is_none() {
                    return Err(invalid("audio gap placement omits its preceding play"));
                }
                if let Some(AudioRepeatValue::Captured { iteration }) = &self.gap_after {
                    let projected = layout.project_scoped(
                        &self.reference.physical,
                        &InstancePath {
                            node: self.reference.physical.clone(),
                            repeats: Vec::new(),
                        },
                        ExactRatio::ZERO,
                        Some(iteration),
                        work.remaining()?,
                    )?;
                    work.spend(projected.work)?;
                }
            }
        }
        let (expected, used) = layout.scoped_repeats(
            scope.root.node(),
            &self.reference.physical,
            work.remaining()?,
        )?;
        work.spend(used)?;
        if !expected.iter().eq(self
            .arguments
            .iter()
            .map(|argument| &argument.reference_repeat))
        {
            return Err(invalid(
                "audio binding reference Repeat path is incomplete or unordered",
            ));
        }
        let mut live = BTreeSet::new();
        for argument in &self.arguments {
            if let AudioRepeatValue::Live { repeat } = &argument.value
                && !live.insert(repeat)
            {
                return Err(invalid("duplicate audio binding live Repeat"));
            }
        }
        let mut clauses = BTreeSet::new();
        let mut previous_root = scope.root.node();
        for clause in &self.births {
            if !clauses.insert(&clause.repeat) {
                return Err(invalid("duplicate audio birth clause"));
            }
            let (_, used) =
                layout.scoped_repeats(previous_root, &clause.definition_root, work.remaining()?)?;
            work.spend(used)?;
            let (_, used) = layout.scoped_repeats(
                &clause.definition_root,
                &self.reference.physical,
                work.remaining()?,
            )?;
            work.spend(used)?;
            previous_root = &clause.definition_root;
            match &clause.survivors {
                AudioBirthSurvivors::CapturedRepeat { repeat } => {
                    work.spend(self.arguments.len())?;
                    let Some(crate::FrozenAudioNode {
                        kind: FrozenAudioKind::Repeat { child, .. },
                        ..
                    }) = layout.nodes().get(repeat)
                    else {
                        return Err(invalid("birth survivors require a captured Repeat"));
                    };
                    if child != &clause.definition_root || !self.arguments.iter().any(|argument| {
                        &argument.reference_repeat == repeat && matches!(&argument.value, AudioRepeatValue::Live { repeat } if repeat == &clause.repeat)
                    }) { return Err(invalid("birth default or captured Repeat argument disagrees")); }
                }
                AudioBirthSurvivors::Run { first, count, .. } => {
                    if *count == 0
                        || u64::from(*first) + u64::from(*count) > u64::from(u32::MAX) + 1
                    {
                        return Err(invalid("audio birth survivor run is invalid"));
                    }
                    if live.contains(&clause.repeat) {
                        return Err(invalid(
                            "explicit birth run cannot replace a captured Repeat's survivors",
                        ));
                    }
                }
            }
        }
        // Every variable argument has an explicit canonical birth route. A
        // concrete override can instead close that argument at capture time.
        for repeat in live {
            if !clauses.contains(repeat) {
                return Err(invalid("live Repeat argument has no birth clause"));
            }
        }
        // Captured arguments must name real effective paths, even if another
        // argument remains variable. Check each selected branch independently.
        for argument in &self.arguments {
            if let AudioRepeatValue::Captured { iteration } = &argument.value {
                let (child, used) = layout.branch_below(
                    &argument.reference_repeat,
                    &self.reference.physical,
                    work.remaining()?,
                )?;
                work.spend(used)?;
                let projected = layout.project_scoped(
                    &argument.reference_repeat,
                    &InstancePath {
                        node: child,
                        repeats: vec![RepeatInstance {
                            node: argument.reference_repeat.clone(),
                            iteration: iteration.clone(),
                        }],
                    },
                    ExactRatio::ZERO,
                    None,
                    work.remaining()?,
                )?;
                work.spend(projected.work)?;
            }
        }
        Ok(())
    }

    pub(crate) fn entry_count(&self) -> usize {
        1 + self.arguments.len() + self.births.len() + usize::from(self.gap_after.is_some())
    }

    fn resolve_with(
        &self,
        layout: &FrozenAudioLayout,
        environment: AudioBindingEnvironment<'_>,
        work: &mut Work,
    ) -> Result<ResolvedAudioPlacement, DocumentError> {
        let before = work.used;
        self.validate_with(layout, work)?;
        let mut birth = None;
        for (index, clause) in self.births.iter().enumerate() {
            work.spend(environment.lookup_work() + 1)?;
            let Some(iteration) = environment.iteration(&clause.repeat) else {
                if environment.excluded(&clause.repeat) {
                    birth = Some(index);
                    continue;
                }
                return Err(invalid("audio binding omits a live Repeat argument"));
            };
            let survives = match &clause.survivors {
                AudioBirthSurvivors::CapturedRepeat { repeat } => {
                    let FrozenAudioKind::Repeat { iterations, .. } = &layout.nodes()[repeat].kind
                    else {
                        unreachable!("validated Repeat")
                    };
                    work.spend(iterations.segment_count())?;
                    iterations.position(iteration).is_some()
                        && layout
                            .overrides()
                            .get(repeat)
                            .is_none_or(|overrides| overrides.get(iteration).is_none())
                }
                AudioBirthSurvivors::Run {
                    allocation,
                    first,
                    count,
                } => {
                    &iteration.allocation == allocation
                        && iteration.ordinal >= *first
                        && u64::from(iteration.ordinal) < u64::from(*first) + u64::from(*count)
                }
            };
            if !survives {
                birth = Some(index);
            }
        }
        let mut gap_birth = false;
        let gap_after = match &self.gap_after {
            Some(AudioRepeatValue::Captured { iteration }) => Some(iteration.clone()),
            Some(AudioRepeatValue::Live { repeat }) => {
                if repeat != &environment.instance().node {
                    return Err(invalid("live gap argument names another Repeat owner"));
                }
                work.spend(environment.lookup_work() + 1)?;
                match environment.gap_after() {
                    Some(iteration) => {
                        let FrozenAudioKind::Repeat {
                            iterations,
                            gap_duration,
                            ..
                        } = &layout.nodes()[&self.reference.physical].kind
                        else {
                            unreachable!("validated gap")
                        };
                        work.spend(iterations.segment_count())?;
                        let survives = *gap_duration != FrameDuration::ZERO
                            && layout
                                .gap_overrides()
                                .get(&self.reference.physical)
                                .is_none_or(|entries| entries.get(iteration).is_none())
                            && iterations
                                .position(iteration)
                                .is_some_and(|position| position + 1 < iterations.len());
                        gap_birth = !survives;
                        survives.then(|| iteration.clone())
                    }
                    None if environment.gap_definition() => {
                        gap_birth = true;
                        None
                    }
                    None => return Err(invalid("audio gap binding omits its live preceding play")),
                }
            }
            None => None,
        };
        let clock = if gap_birth {
            AudioClockRoot::GapDefinitionPointCeil {
                repeat: self.reference.physical.clone(),
            }
        } else {
            birth.map_or_else(
                || self.reference.root.clone(),
                |index| AudioClockRoot::DefinitionPointCeil {
                    root: self.births[index].definition_root.clone(),
                },
            )
        };
        let scope = clock_scope(layout, &clock)?;
        let (required, used) = layout.scoped_repeats(
            scope.root.node(),
            &self.reference.physical,
            work.remaining()?,
        )?;
        work.spend(used)?;
        let mut repeats = Vec::with_capacity(required.len());
        for reference_repeat in required {
            work.spend(self.arguments.len() + environment.lookup_work() + 1)?;
            let argument = self
                .arguments
                .iter()
                .find(|argument| argument.reference_repeat == reference_repeat)
                .ok_or_else(|| invalid("missing reference Repeat argument"))?;
            let iteration = match &argument.value {
                AudioRepeatValue::Captured { iteration } => iteration,
                AudioRepeatValue::Live { repeat } => environment
                    .iteration(repeat)
                    .ok_or_else(|| invalid("definition omitted an inner Repeat argument"))?,
            };
            repeats.push(RepeatInstance {
                node: reference_repeat,
                iteration: iteration.clone(),
            });
        }
        let instance = InstancePath {
            node: self.reference.physical.clone(),
            repeats,
        };
        let (projection, local_support) =
            scope.project(layout, &instance, gap_after.as_ref(), false, work)?;
        let mut local_support = local_support.expect("meaningful projection retains empty ranges");
        if let Some(support) = &scope.support {
            let selected = support
                .start
                .checked_sub(projection.origin)?
                .checked_div(projection.frames_per_local_frame)?
                ..support
                    .end
                    .checked_sub(projection.origin)?
                    .checked_div(projection.frames_per_local_frame)?;
            crate::audio_reference::clip_binding_support(&mut local_support, selected)?;
        }
        let origin = projection.origin.checked_add(
            self.reference_local_offset
                .checked_mul(projection.frames_per_local_frame)?,
        )?;
        let local_support = local_support
            .start
            .checked_sub(self.reference_local_offset)?
            ..local_support.end.checked_sub(self.reference_local_offset)?;
        let rate = layout.rate();
        Ok(ResolvedAudioPlacement {
            grid_rule: scope.rule,
            grid_origin: scope.grid_origin,
            frames_per_sample: ExactRatio::new(
                i128::from(rate.numerator()),
                i128::from(MIX_SAMPLE_RATE) * i128::from(rate.denominator()),
            )?,
            origin,
            frames_per_local_frame: projection.frames_per_local_frame,
            local_duration: projection.local_duration,
            local_support,
            instance,
            gap_after,
            clock,
            birth,
            gap_birth,
            work: work.used - before,
        })
    }
}

impl AudioReanchorStep {
    pub fn for_allocation(
        placement: AudioPlacementTemplate,
        window: Option<ExactFrameRange>,
    ) -> Self {
        Self {
            placement,
            window,
            anchor: AudioReanchorAnchor::AllocationEntry,
        }
    }

    pub fn for_source_endpoint(
        placement: AudioPlacementTemplate,
        endpoint: AudioSourceEndpoint,
    ) -> Self {
        Self {
            placement,
            window: None,
            anchor: AudioReanchorAnchor::SourceEndpoint { endpoint },
        }
    }

    fn validate_anchor(&self, layout: &FrozenAudioLayout) -> Result<(), DocumentError> {
        if self.anchor.is_allocation_entry() {
            return Ok(());
        }
        if self.window.is_some()
            || self.placement.reference.recipe != AudioRecipeKind::Node
            || self.placement.gap_after.is_some()
            || !matches!(
                layout
                    .nodes()
                    .get(&self.placement.reference.physical)
                    .map(|node| &node.kind),
                Some(FrozenAudioKind::Source { .. })
            )
        {
            return Err(invalid(
                "Source endpoint requires a Source node clock without a window",
            ));
        }
        Ok(())
    }

    fn allocation_entry(
        &self,
        layout: &FrozenAudioLayout,
        placement: &ResolvedAudioPlacement,
        work: &mut Work,
    ) -> Result<Option<ExactRatio>, DocumentError> {
        self.validate_anchor(layout)?;
        let scope = clock_scope(layout, &placement.clock)?;
        let (projection, mut allocation) = scope.project(
            layout,
            &placement.instance,
            placement.gap_after.as_ref(),
            true,
            work,
        )?;
        let captured = clock_scope(layout, &self.placement.reference.root)?;
        // A later outer wrapper can select the same definition root. Such a
        // birth keeps this root's intrinsic window; only a narrower root drops
        // a window authored in its enclosing captured scope.
        let window = self.window.filter(|_| captured.root == scope.root);
        for constraint in scope
            .support
            .into_iter()
            .chain(window.map(|range| range.start..range.end))
        {
            let Some(range) = &mut allocation else { break };
            let local = constraint
                .start
                .checked_sub(projection.origin)?
                .checked_div(projection.frames_per_local_frame)?
                ..constraint
                    .end
                    .checked_sub(projection.origin)?
                    .checked_div(projection.frames_per_local_frame)?;
            crate::audio_reference::clip_binding_support(range, local)?;
            if range.start == range.end {
                allocation = None;
            }
        }
        // The frozen projection and its window still use historical local
        // coordinates. Resume evaluation consumes the current physical clock.
        let entry = match (self.anchor, allocation) {
            (AudioReanchorAnchor::AllocationEntry, range) => range.map(|range| range.start),
            (AudioReanchorAnchor::SourceEndpoint { endpoint }, Some(range))
                if range.start.compare(range.end).is_lt() =>
            {
                Some(match endpoint {
                    AudioSourceEndpoint::Start => range.start,
                    AudioSourceEndpoint::End => range.end,
                })
            }
            (AudioReanchorAnchor::SourceEndpoint { .. }, _) => {
                return Err(invalid(
                    "Source endpoint requires a positive captured allocation",
                ));
            }
        };
        entry
            .map(|point| {
                point
                    .checked_sub(self.placement.reference_local_offset)
                    .map_err(DocumentError::from)
            })
            .transpose()
    }
}

impl AudioBindingState {
    pub fn new(
        timings: Vec<AudioTimingRecord>,
        bindings: BTreeMap<NodeId, OwnedAudioBinding>,
    ) -> Result<Self, DocumentError> {
        Self::new_with_gaps(timings, bindings, BTreeMap::new())
    }

    pub fn new_with_gaps(
        timings: Vec<AudioTimingRecord>,
        bindings: BTreeMap<NodeId, OwnedAudioBinding>,
        gap_bindings: BTreeMap<NodeId, OwnedAudioBinding>,
    ) -> Result<Self, DocumentError> {
        if timings.len() > MAX_AUDIO_BINDING_ENTRIES {
            return Err(limit("audio timing record count"));
        }
        let mut unique = BTreeMap::new();
        for timing in timings {
            if unique.insert(timing.id, timing.layout).is_some() {
                return Err(invalid("duplicate audio timing identity"));
            }
        }
        let state = Self {
            timings: unique,
            bindings,
            gap_bindings,
        };
        state.to_json()?;
        Ok(state)
    }
    pub fn is_empty(&self) -> bool {
        self.timings.is_empty() && self.bindings.is_empty() && self.gap_bindings.is_empty()
    }
    pub fn timings(&self) -> &BTreeMap<AudioTimingId, FrozenAudioLayout> {
        &self.timings
    }
    pub fn bindings(&self) -> &BTreeMap<NodeId, OwnedAudioBinding> {
        &self.bindings
    }
    pub fn gap_bindings(&self) -> &BTreeMap<NodeId, OwnedAudioBinding> {
        &self.gap_bindings
    }
    pub(crate) fn owners(
        &self,
    ) -> impl Iterator<Item = (AudioRecipeKind, &NodeId, &OwnedAudioBinding)> {
        self.bindings
            .iter()
            .map(|(owner, binding)| (AudioRecipeKind::Node, owner, binding))
            .chain(
                self.gap_bindings
                    .iter()
                    .map(|(owner, binding)| (AudioRecipeKind::RepeatGap, owner, binding)),
            )
    }

    /// Allocation names remain reserved even when no current Repeat uses them.
    pub fn allocation_ids(&self) -> BTreeSet<&RevisionId> {
        let mut ids = BTreeSet::new();
        for (identity, layout) in &self.timings {
            ids.insert(&identity.allocation);
            for node in layout.nodes().values() {
                if let FrozenAudioKind::Repeat { iterations, .. } = &node.kind {
                    for (allocation, _, _) in iterations.segments() {
                        ids.insert(allocation);
                    }
                }
            }
            for lineage in layout.audio_lineage().values() {
                ids.insert(&lineage.allocation);
            }
        }
        for (_, _, binding) in self.owners() {
            for template in binding.placements() {
                ids.insert(&template.reference.timing.allocation);
                for value in template
                    .arguments
                    .iter()
                    .map(|argument| &argument.value)
                    .chain(template.gap_after.iter())
                {
                    if let AudioRepeatValue::Captured { iteration } = value {
                        ids.insert(&iteration.allocation);
                    }
                }
                for clause in &template.births {
                    if let AudioBirthSurvivors::Run { allocation, .. } = &clause.survivors {
                        ids.insert(allocation);
                    }
                }
            }
        }
        ids
    }

    pub fn validate(&self) -> Result<(), DocumentError> {
        self.validation_work().map(|_| ())
    }

    fn validation_work(&self) -> Result<Work, DocumentError> {
        let mut work = Work::new(MAX_AUDIO_BINDING_ENTRIES)?;
        if self.timings.len() > MAX_AUDIO_BINDING_ENTRIES
            || self.bindings.len() > MAX_AUDIO_BINDING_ENTRIES
            || self.gap_bindings.len() > MAX_AUDIO_BINDING_ENTRIES
        {
            return Err(limit("audio binding record count"));
        }
        let mut nodes = 0usize;
        let mut runs = 0usize;
        for layout in self.timings.values() {
            work.spend(layout.nodes().len() + layout.audio_lineage().len())?;
            nodes = nodes
                .checked_add(layout.nodes().len())
                .ok_or_else(|| limit("audio timing node overflow"))?;
            for node in layout.nodes().values() {
                if let FrozenAudioKind::Repeat { iterations, .. } = &node.kind {
                    work.spend(iterations.segment_count())?;
                    runs = runs
                        .checked_add(iterations.segment_count())
                        .ok_or_else(|| limit("audio timing run overflow"))?;
                }
            }
            if nodes > MAX_AUDIO_BINDING_ENTRIES || runs > MAX_AUDIO_BINDING_ENTRIES {
                return Err(limit("aggregate audio timing complexity"));
            }
            layout.validate()?;
        }
        let mut used = BTreeSet::new();
        let mut entries = 0usize;
        for (kind, _, binding) in self.owners() {
            binding_wire_size(binding)?;
            let terms = binding
                .resume
                .as_ref()
                .map_or(&[][..], |resume| resume.phase.terms.as_slice());
            if terms
                .len()
                .checked_add(binding.reanchors.len())
                .is_none_or(|count| count > MAX_AUDIO_BINDING_TERMS)
            {
                return Err(limit("audio phase term and reanchor count"));
            }
            for step in &binding.reanchors {
                if let Some(window) = step.window {
                    ExactFrameRange::new(window.start, window.end)?;
                }
                let layout = self
                    .timings
                    .get(&step.placement.reference.timing)
                    .ok_or_else(|| invalid("audio timing identity is missing"))?;
                step.validate_anchor(layout)?;
            }
            for template in binding.placements() {
                if template.reference.recipe != kind
                    && !(kind == AudioRecipeKind::Node
                        && template.reference.recipe == AudioRecipeKind::RepeatGap
                        && !matches!(template.gap_after, Some(AudioRepeatValue::Live { .. })))
                {
                    return Err(invalid(
                        "audio binding placement recipe disagrees with its owner kind",
                    ));
                }
                entries = entries
                    .checked_add(template.entry_count())
                    .ok_or_else(|| limit("audio binding entry overflow"))?;
                if entries > MAX_AUDIO_BINDING_ENTRIES {
                    return Err(limit("aggregate audio binding entries"));
                }
                let layout = self
                    .timings
                    .get(&template.reference.timing)
                    .ok_or_else(|| invalid("audio timing identity is missing"))?;
                template.validate_with(layout, &mut work)?;
                used.insert(&template.reference.timing);
            }
        }
        if used.len() != self.timings.len() {
            return Err(invalid("unreferenced audio timing record"));
        }
        Ok(work)
    }

    pub fn validate_for(&self, document: &ProjectDocument) -> Result<(), DocumentError> {
        if self.is_empty() {
            return Ok(());
        }
        let mut work = self.validation_work()?;
        if self
            .timings
            .values()
            .any(|layout| layout.rate() != document.presentation_basis().frame_rate)
        {
            return Err(invalid("audio timing rate differs from the project rate"));
        }
        let mut parents = BTreeMap::new();
        for parent in document.nodes().keys() {
            work.spend(1)?;
            for child in document.children(parent) {
                parents.insert(child, parent);
            }
        }
        for (kind, owner, binding) in self.owners() {
            let node = document
                .nodes()
                .get(owner)
                .ok_or_else(|| invalid("audio binding owner is missing"))?;
            if binding
                .reanchors
                .iter()
                .any(|step| !step.anchor.is_allocation_entry())
                && (kind != AudioRecipeKind::Node || !matches!(node.kind, NodeKind::Source { .. }))
            {
                return Err(invalid(
                    "Source endpoint binding requires a current Source owner",
                ));
            }
            if kind == AudioRecipeKind::RepeatGap && !matches!(node.kind, NodeKind::Repeat { .. }) {
                return Err(invalid("gap binding owner is not a Repeat"));
            }
            if kind == AudioRecipeKind::Node
                && !matches!(node.kind, NodeKind::Source { .. } | NodeKind::Hold { .. })
                && !matches!(&node.kind, NodeKind::Retime { duration, mapping, pitch: PitchPolicy::Preserve, .. } if mapping.duration() != *duration)
            {
                return Err(invalid("audio binding owner is not a physical recipe"));
            }
            let duration = match &node.kind {
                NodeKind::Source { source } => source.duration,
                NodeKind::Hold { recipe } => recipe.duration,
                NodeKind::Retime { duration, .. } => *duration,
                NodeKind::Repeat { gap: Some(gap), .. }
                    if kind == AudioRecipeKind::RepeatGap
                        && gap.duration != FrameDuration::ZERO =>
                {
                    gap.duration
                }
                _ => {
                    return Err(invalid(
                        "audio binding owner has no positive physical recipe",
                    ));
                }
            };
            if kind == AudioRecipeKind::Node
                && !matches!(node.kind, NodeKind::Hold { .. })
                && binding
                    .placements()
                    .any(|template| template.reference.recipe == AudioRecipeKind::RepeatGap)
            {
                return Err(invalid(
                    "a retained gap clock requires a current Hold owner",
                ));
            }
            // A shortened gap keeps its affine anchor; current raw support,
            // rather than moving that retained coordinate, bounds its output.
            if binding.resume.as_ref().is_some_and(|resume| {
                resume.local_boundary.compare_integer(0).is_lt()
                    || (kind == AudioRecipeKind::Node
                        && binding.lattice.reference.recipe == AudioRecipeKind::Node
                        && resume
                            .local_boundary
                            .compare_integer(duration.frames())
                            .is_gt())
            }) {
                return Err(invalid(
                    "audio resume boundary is outside its physical owner",
                ));
            }
            let mut ancestors = Vec::new();
            let mut node = owner;
            let mut depth = 0usize;
            while let Some(parent) = parents.get(node) {
                work.spend(1)?;
                if depth >= MAX_DOCUMENT_DEPTH {
                    return Err(limit("live audio binding depth"));
                }
                depth += 1;
                if matches!(document.nodes()[*parent].kind, NodeKind::Repeat { .. }) {
                    ancestors.push(*parent);
                }
                node = parent;
            }
            ancestors.reverse();
            for template in binding.placements() {
                if let Some(AudioRepeatValue::Live { repeat }) = &template.gap_after
                    && repeat != owner
                {
                    return Err(invalid("live gap argument names another owner"));
                }
                let mut last = None;
                for clause in &template.births {
                    work.spend(ancestors.len())?;
                    let position = ancestors
                        .iter()
                        .position(|repeat| *repeat == &clause.repeat)
                        .ok_or_else(|| {
                            invalid("audio birth owner is not a live Repeat ancestor")
                        })?;
                    if last.is_some_and(|last| position <= last) {
                        return Err(invalid("live audio birth clauses are out of order"));
                    }
                    last = Some(position);
                }
                for argument in &template.arguments {
                    work.spend(ancestors.len())?;
                    if let AudioRepeatValue::Live { repeat } = &argument.value
                        && !ancestors.contains(&repeat)
                    {
                        return Err(invalid("audio argument is not a live Repeat ancestor"));
                    }
                }
            }
        }
        Ok(())
    }

    pub fn resolve(
        &self,
        owner: &NodeId,
        live: &InstancePath,
        maximum_work: usize,
    ) -> Result<ResolvedAudioBinding, DocumentError> {
        self.resolve_in(
            owner,
            AudioBindingEnvironment::Occurrence(live),
            maximum_work,
        )
    }

    pub fn resolve_in(
        &self,
        owner: &NodeId,
        environment: AudioBindingEnvironment<'_>,
        maximum_work: usize,
    ) -> Result<ResolvedAudioBinding, DocumentError> {
        self.resolve_owner(owner, environment, maximum_work, AudioRecipeKind::Node)
    }

    pub fn resolve_gap_in(
        &self,
        owner: &NodeId,
        environment: AudioBindingEnvironment<'_>,
        maximum_work: usize,
    ) -> Result<ResolvedAudioBinding, DocumentError> {
        self.resolve_owner(owner, environment, maximum_work, AudioRecipeKind::RepeatGap)
    }

    fn resolve_owner(
        &self,
        owner: &NodeId,
        environment: AudioBindingEnvironment<'_>,
        maximum_work: usize,
        kind: AudioRecipeKind,
    ) -> Result<ResolvedAudioBinding, DocumentError> {
        environment.validate()?;
        if (kind == AudioRecipeKind::RepeatGap)
            != matches!(
                environment,
                AudioBindingEnvironment::GapOccurrence { .. }
                    | AudioBindingEnvironment::GapDefinition { .. }
            )
        {
            return Err(invalid("audio binding environment has another recipe kind"));
        }
        if &environment.instance().node != owner {
            return Err(invalid("audio binding occurrence names another owner"));
        }
        let bindings = match kind {
            AudioRecipeKind::Node => &self.bindings,
            AudioRecipeKind::RepeatGap => &self.gap_bindings,
        };
        let binding = bindings
            .get(owner)
            .ok_or_else(|| invalid("audio binding owner is missing"))?;
        let mut work = Work::new(maximum_work)?;
        work.spend(environment.instance().repeats.len() + 1)?;
        let resolve = |template: &AudioPlacementTemplate, work: &mut Work| {
            let layout = self
                .timings
                .get(&template.reference.timing)
                .ok_or_else(|| invalid("audio timing identity is missing"))?;
            template.resolve_with(layout, environment, work)
        };
        let lattice = resolve(&binding.lattice, &mut work)?;
        let mut resume = binding
            .resume
            .as_ref()
            .map(|resume| {
                let mut delta = resume.phase.constant;
                for term in &resume.phase.terms {
                    work.spend(1)?;
                    let placement = resolve(&term.placement, &mut work)?;
                    let from = placement.sample_boundary(term.from_local)?;
                    let to = placement.sample_boundary(term.to_local)?;
                    let distance = ExactRatio::new(i128::from(to) - i128::from(from), 1)?;
                    delta = delta
                        .checked_add(distance.checked_mul(placement.local_frames_per_sample()?)?)?;
                }
                Ok::<_, DocumentError>(ResolvedAudioResume {
                    local_boundary: resume.local_boundary,
                    reference_local_delta: delta,
                })
            })
            .transpose()?;
        for step in &binding.reanchors {
            work.spend(1)?;
            let placement = resolve(&step.placement, &mut work)?;
            let layout = &self.timings[&step.placement.reference.timing];
            let Some(entry) = step.allocation_entry(layout, &placement, &mut work)? else {
                // Hidden retained context has no new anchor. A later edit may
                // expose it, at which point the previous map must still apply.
                continue;
            };
            let previous = resume.get_or_insert(ResolvedAudioResume {
                local_boundary: lattice.local_support.start,
                reference_local_delta: ExactRatio::ZERO,
            });
            let from = placement.sample_boundary(previous.local_boundary)?;
            let to = placement.sample_boundary(entry)?;
            let distance = ExactRatio::new(i128::from(to) - i128::from(from), 1)?;
            previous.reference_local_delta = previous
                .reference_local_delta
                .checked_add(distance.checked_mul(placement.local_frames_per_sample()?)?)?;
            previous.local_boundary = entry;
        }
        Ok(ResolvedAudioBinding {
            lattice,
            resume,
            work: work.used,
        })
    }

    pub fn from_json(json: &str) -> Result<Self, DocumentError> {
        if json.len() > MAX_DOCUMENT_JSON_BYTES {
            return Err(limit("audio binding JSON byte limit"));
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire<'a> {
            #[serde(borrow)]
            timings: &'a RawValue,
            #[serde(borrow)]
            bindings: &'a RawValue,
            #[serde(default, borrow, deserialize_with = "present_raw")]
            gap_bindings: Option<&'a RawValue>,
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Timing<'a> {
            id: AudioTimingId,
            #[serde(borrow)]
            layout: &'a RawValue,
        }
        let wire: Wire<'_> = serde_json::from_str(json).map_err(DocumentError::json)?;
        let raws: Vec<&RawValue> =
            bounded_json_sequence(wire.timings.get(), MAX_AUDIO_BINDING_ENTRIES)?;
        let mut timing_wires = Vec::with_capacity(raws.len());
        let mut budget = Work::new(MAX_AUDIO_BINDING_ENTRIES)?;
        for raw in raws {
            let value: Timing<'_> = serde_json::from_str(raw.get()).map_err(DocumentError::json)?;
            let (nodes, runs, lineages) =
                FrozenAudioLayout::preflight_binding_counts(value.layout.get())?;
            budget.spend(nodes + runs + lineages)?;
            timing_wires.push(value);
        }
        let raws: BTreeMap<NodeId, &RawValue> = bounded_json_map(wire.bindings.get())?;
        let gaps = wire
            .gap_bindings
            .map(|raw| bounded_json_map(raw.get()))
            .transpose()?
            .unwrap_or_default();
        for raw in raws.values().chain(gaps.values()) {
            preflight_binding(raw.get(), &mut budget)?;
        }
        // All aggregate collection counts are charged before any frozen tree
        // or binding body is materialized.
        let mut timings = Vec::with_capacity(timing_wires.len());
        for value in timing_wires {
            timings.push(AudioTimingRecord {
                id: value.id,
                layout: FrozenAudioLayout::from_json(value.layout.get())?,
            });
        }
        let mut bindings = BTreeMap::new();
        for (owner, raw) in raws {
            bindings.insert(
                owner,
                serde_json::from_str(raw.get()).map_err(DocumentError::json)?,
            );
        }
        let mut gap_bindings = BTreeMap::new();
        for (owner, raw) in gaps {
            gap_bindings.insert(
                owner,
                serde_json::from_str(raw.get()).map_err(DocumentError::json)?,
            );
        }
        Self::new_with_gaps(timings, bindings, gap_bindings)
    }

    pub fn to_json(&self) -> Result<String, DocumentError> {
        self.validate()?;
        let mut output = BoundedJson::default();
        serde_json::to_writer(&mut output, self).map_err(|error| {
            if output.exceeded {
                limit("audio binding JSON byte limit")
            } else {
                DocumentError::json(error)
            }
        })?;
        String::from_utf8(output.bytes).map_err(|error| invalid(&error.to_string()))
    }
}

impl Serialize for AudioBindingState {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        struct Timings<'a>(&'a BTreeMap<AudioTimingId, FrozenAudioLayout>);
        impl Serialize for Timings<'_> {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                #[derive(Serialize)]
                struct Record<'a> {
                    id: &'a AudioTimingId,
                    layout: &'a FrozenAudioLayout,
                }
                let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
                for (id, layout) in self.0 {
                    sequence.serialize_element(&Record { id, layout })?;
                }
                sequence.end()
            }
        }
        let mut state = serializer.serialize_struct(
            "AudioBindingState",
            2 + usize::from(!self.gap_bindings.is_empty()),
        )?;
        state.serialize_field("timings", &Timings(&self.timings))?;
        state.serialize_field("bindings", &self.bindings)?;
        if !self.gap_bindings.is_empty() {
            state.serialize_field("gap_bindings", &self.gap_bindings)?;
        }
        state.end()
    }
}

impl<'de> Deserialize<'de> for AudioBindingState {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = Box::<RawValue>::deserialize(deserializer)?;
        Self::from_json(raw.get()).map_err(de::Error::custom)
    }
}

/// Raw values keep tagged enums and identifiers borrowed until collection
/// admission is complete. The subsequent strict typed pass owns vocabulary.
fn preflight_binding(json: &str, budget: &mut Work) -> Result<(), DocumentError> {
    if compact_json_bytes(json) > MAX_BINDING_WIRE_BYTES {
        return Err(limit("individual audio binding byte limit"));
    }
    #[derive(Deserialize)]
    struct Binding<'a> {
        #[serde(borrow)]
        lattice: &'a RawValue,
        #[serde(borrow)]
        resume: Option<&'a RawValue>,
        #[serde(borrow)]
        reanchors: Option<&'a RawValue>,
    }
    #[derive(Deserialize)]
    struct Resume<'a> {
        #[serde(borrow)]
        phase: &'a RawValue,
    }
    #[derive(Deserialize)]
    struct Phase<'a> {
        #[serde(borrow)]
        terms: &'a RawValue,
    }
    #[derive(Deserialize)]
    struct Term<'a> {
        #[serde(borrow)]
        placement: &'a RawValue,
    }
    #[derive(Deserialize)]
    struct Template<'a> {
        #[serde(borrow)]
        arguments: &'a RawValue,
        #[serde(borrow)]
        births: &'a RawValue,
        #[serde(default, borrow, deserialize_with = "present_raw")]
        gap_after: Option<&'a RawValue>,
    }
    let mut template = |raw: &RawValue| -> Result<(), DocumentError> {
        let value: Template<'_> = serde_json::from_str(raw.get()).map_err(DocumentError::json)?;
        let args: Vec<&RawValue> =
            bounded_json_sequence(value.arguments.get(), MAX_DOCUMENT_DEPTH)?;
        let births: Vec<&RawValue> = bounded_json_sequence(value.births.get(), MAX_DOCUMENT_DEPTH)?;
        budget.spend(1 + args.len() + births.len() + usize::from(value.gap_after.is_some()))
    };
    let binding: Binding<'_> = serde_json::from_str(json).map_err(DocumentError::json)?;
    template(binding.lattice)?;
    let mut term_count = 0;
    if let Some(raw) = binding.resume {
        let resume: Resume<'_> = serde_json::from_str(raw.get()).map_err(DocumentError::json)?;
        let phase: Phase<'_> =
            serde_json::from_str(resume.phase.get()).map_err(DocumentError::json)?;
        let terms: Vec<&RawValue> =
            bounded_json_sequence(phase.terms.get(), MAX_AUDIO_BINDING_TERMS)?;
        term_count = terms.len();
        for raw in terms {
            let term: Term<'_> = serde_json::from_str(raw.get()).map_err(DocumentError::json)?;
            template(term.placement)?;
        }
    }
    if let Some(raw) = binding.reanchors {
        let steps: Vec<&RawValue> =
            bounded_json_sequence(raw.get(), MAX_AUDIO_BINDING_TERMS - term_count)?;
        for raw in steps {
            let step: Term<'_> = serde_json::from_str(raw.get()).map_err(DocumentError::json)?;
            template(step.placement)?;
        }
    }
    Ok(())
}

fn present_raw<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<&'de RawValue>, D::Error> {
    <&RawValue>::deserialize(deserializer).map(Some)
}

/// Pretty document serialization changes indentation, never the string bytes.
/// Charge all bytes inside strings, including escapes, without allocating a
/// canonical copy. The containing state still enforces its raw JSON byte cap.
fn compact_json_bytes(json: &str) -> usize {
    let mut count = 0;
    let mut in_string = false;
    let mut escaped = false;
    for byte in json.bytes() {
        if in_string {
            count += 1;
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
        } else if !matches!(byte, b' ' | b'\t' | b'\r' | b'\n') {
            count += 1;
            in_string = byte == b'"';
        }
    }
    count
}

fn binding_wire_size(binding: &OwnedAudioBinding) -> Result<(), DocumentError> {
    struct Count(usize);
    impl Write for Count {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0 = self
                .0
                .checked_add(bytes.len())
                .filter(|count| *count <= MAX_BINDING_WIRE_BYTES)
                .ok_or_else(|| io::Error::other("individual audio binding byte limit"))?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(&mut Count(0), binding)
        .map_err(|_| limit("individual audio binding byte limit"))
}

fn path_vec<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Vec<T>, D::Error> {
    BoundedSequence::<T> {
        maximum: MAX_DOCUMENT_DEPTH,
        marker: std::marker::PhantomData,
    }
    .deserialize(deserializer)
}
fn term_vec<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Vec<T>, D::Error> {
    BoundedSequence::<T> {
        maximum: MAX_AUDIO_BINDING_TERMS,
        marker: std::marker::PhantomData,
    }
    .deserialize(deserializer)
}
struct BoundedSequence<T> {
    maximum: usize,
    marker: std::marker::PhantomData<T>,
}
impl<'de, T: Deserialize<'de>> DeserializeSeed<'de> for BoundedSequence<T> {
    type Value = Vec<T>;
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_seq(self)
    }
}
impl<'de, T: Deserialize<'de>> Visitor<'de> for BoundedSequence<T> {
    type Value = Vec<T>;
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a bounded sequence")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Self::Value, A::Error> {
        let mut values = Vec::new();
        loop {
            if values.len() == self.maximum {
                if sequence.next_element::<IgnoredAny>()?.is_some() {
                    return Err(de::Error::custom("audio binding sequence limit"));
                }
                break;
            }
            let Some(value) = sequence.next_element()? else {
                break;
            };
            values.push(value);
        }
        Ok(values)
    }
}
fn bounded_json_sequence<'a, T: Deserialize<'a>>(
    json: &'a str,
    maximum: usize,
) -> Result<Vec<T>, DocumentError> {
    let mut decoder = serde_json::Deserializer::from_str(json);
    let values = BoundedSequence {
        maximum,
        marker: std::marker::PhantomData,
    }
    .deserialize(&mut decoder)
    .map_err(DocumentError::json)?;
    decoder.end().map_err(DocumentError::json)?;
    Ok(values)
}
fn bounded_json_map(json: &str) -> Result<BTreeMap<NodeId, &RawValue>, DocumentError> {
    struct Map;
    impl<'de> Visitor<'de> for Map {
        type Value = BTreeMap<NodeId, &'de RawValue>;
        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("bounded unique audio bindings")
        }
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
            let mut values = BTreeMap::new();
            while let Some(owner) = map.next_key::<NodeId>()? {
                if values.len() == MAX_AUDIO_BINDING_ENTRIES {
                    return Err(de::Error::custom("audio binding map limit"));
                }
                if values.contains_key(&owner) {
                    return Err(de::Error::custom("duplicate audio binding owner"));
                }
                values.insert(owner, map.next_value()?);
            }
            Ok(values)
        }
    }
    let mut decoder = serde_json::Deserializer::from_str(json);
    let values = decoder.deserialize_map(Map).map_err(DocumentError::json)?;
    decoder.end().map_err(DocumentError::json)?;
    Ok(values)
}

#[derive(Default)]
struct BoundedJson {
    bytes: Vec<u8>,
    exceeded: bool,
}
impl Write for BoundedJson {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self
            .bytes
            .len()
            .checked_add(bytes.len())
            .is_none_or(|size| size > MAX_DOCUMENT_JSON_BYTES)
        {
            self.exceeded = true;
            return Err(io::Error::other("audio binding JSON byte limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn invalid(message: &str) -> DocumentError {
    DocumentError::new(DocumentErrorCode::InvalidTree, message)
}
fn limit(message: &str) -> DocumentError {
    DocumentError::new(DocumentErrorCode::LimitExceeded, message)
}
