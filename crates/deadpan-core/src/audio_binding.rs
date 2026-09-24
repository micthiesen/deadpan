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
    DocumentError, DocumentErrorCode, ExactRatio, FrameDuration, FrozenAudioKind,
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioReferenceClock {
    pub timing: AudioTimingId,
    pub root: AudioClockRoot,
    pub physical: NodeId,
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
    #[serde(deserialize_with = "path_vec")]
    pub arguments: Vec<AudioRepeatArgument>,
    /// Outer-to-inner lexical order. The innermost matching birth wins.
    #[serde(deserialize_with = "path_vec")]
    pub births: Vec<AudioBirthClause>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnedAudioBinding {
    pub lattice: AudioPlacementTemplate,
    pub resume: Option<AudioResume>,
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
}

impl AudioBindingEnvironment<'_> {
    fn lookup_work(&self) -> usize {
        self.instance().repeats.len()
            + match self {
                Self::Occurrence(_) => 0,
                Self::Definition {
                    outside_repeats, ..
                } => outside_repeats.len(),
            }
    }
    fn instance(&self) -> &InstancePath {
        match self {
            Self::Occurrence(instance) | Self::Definition { instance, .. } => instance,
        }
    }
    fn excluded(&self, repeat: &NodeId) -> bool {
        matches!(self, Self::Definition { outside_repeats, .. } if outside_repeats.contains(repeat))
    }
    fn validate(&self) -> Result<(), DocumentError> {
        self.instance().validate_depth()?;
        let mut seen = BTreeSet::new();
        for repeat in &self.instance().repeats {
            if !seen.insert(&repeat.node) {
                return Err(invalid("duplicate live Repeat argument"));
            }
        }
        if let Self::Definition {
            outside_repeats, ..
        } = self
        {
            if outside_repeats.len() > MAX_DOCUMENT_DEPTH {
                return Err(limit("definition exclusion depth"));
            }
            for repeat in *outside_repeats {
                if !seen.insert(repeat) {
                    return Err(invalid("duplicate or present excluded Repeat"));
                }
            }
        }
        Ok(())
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
    pub local_duration: FrameDuration,
    /// Meaningful captured Edit/clock support in physical-local frames. Source
    /// placement and audibility still come from the current owned recipe.
    pub local_support: std::ops::Range<ExactRatio>,
    pub instance: InstancePath,
    pub birth: Option<usize>,
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
    root: &'a NodeId,
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
            root: layout.root(),
            grid_origin: ExactRatio::ZERO,
            rule: AudioBindingGridRule::RootRoundEven,
            support: None,
        }),
        AudioClockRoot::DefinitionPointCeil { root } => {
            if !layout.nodes().contains_key(root) {
                return Err(invalid("definition clock root is missing"));
            }
            Ok(ClockScope {
                root,
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
                root: child,
                grid_origin: ExactRatio::integer(mapping.start().0),
                rule: AudioBindingGridRule::PointCeil,
                support: Some(
                    ExactRatio::integer(mapping.start().0)..ExactRatio::integer(mapping.end().0),
                ),
            })
        }
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
        work.spend(self.arguments.len() + self.births.len() + 1)?;
        let scope = clock_scope(layout, &self.reference.root)?;
        let target = layout
            .nodes()
            .get(&self.reference.physical)
            .ok_or_else(|| invalid("audio binding physical alias is missing"))?;
        if !physical(&target.kind, target.duration) {
            return Err(invalid("audio binding requires a physical recipe"));
        }
        let (expected, used) =
            layout.scoped_repeats(scope.root, &self.reference.physical, work.remaining()?)?;
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
        let mut previous_root = scope.root;
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
        let clock = birth.map_or_else(
            || self.reference.root.clone(),
            |index| AudioClockRoot::DefinitionPointCeil {
                root: self.births[index].definition_root.clone(),
            },
        );
        let scope = clock_scope(layout, &clock)?;
        let (required, used) =
            layout.scoped_repeats(scope.root, &self.reference.physical, work.remaining()?)?;
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
        let (projection, mut local_support) =
            layout.project_scoped_supported(scope.root, &instance, work.remaining()?)?;
        work.spend(projection.work)?;
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
        let rate = layout.rate();
        Ok(ResolvedAudioPlacement {
            grid_rule: scope.rule,
            grid_origin: scope.grid_origin,
            frames_per_sample: ExactRatio::new(
                i128::from(rate.numerator()),
                i128::from(MIX_SAMPLE_RATE) * i128::from(rate.denominator()),
            )?,
            origin: projection.origin,
            frames_per_local_frame: projection.frames_per_local_frame,
            local_duration: projection.local_duration,
            local_support,
            instance,
            clock,
            birth,
            work: work.used - before,
        })
    }
}

impl AudioBindingState {
    pub fn new(
        timings: Vec<AudioTimingRecord>,
        bindings: BTreeMap<NodeId, OwnedAudioBinding>,
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
        };
        state.to_json()?;
        Ok(state)
    }
    pub fn is_empty(&self) -> bool {
        self.timings.is_empty() && self.bindings.is_empty()
    }
    pub fn timings(&self) -> &BTreeMap<AudioTimingId, FrozenAudioLayout> {
        &self.timings
    }
    pub fn bindings(&self) -> &BTreeMap<NodeId, OwnedAudioBinding> {
        &self.bindings
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
        for binding in self.bindings.values() {
            let terms = binding
                .resume
                .as_ref()
                .map_or(&[][..], |resume| resume.phase.terms.as_slice());
            for template in
                std::iter::once(&binding.lattice).chain(terms.iter().map(|term| &term.placement))
            {
                ids.insert(&template.reference.timing.allocation);
                for argument in &template.arguments {
                    if let AudioRepeatValue::Captured { iteration } = &argument.value {
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
        for binding in self.bindings.values() {
            binding_wire_size(binding)?;
            let terms = binding
                .resume
                .as_ref()
                .map_or(&[][..], |resume| resume.phase.terms.as_slice());
            if terms.len() > MAX_AUDIO_BINDING_TERMS {
                return Err(limit("audio phase term count"));
            }
            for template in
                std::iter::once(&binding.lattice).chain(terms.iter().map(|term| &term.placement))
            {
                entries = entries
                    .checked_add(1 + template.arguments.len() + template.births.len())
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
        for (owner, binding) in &self.bindings {
            let node = document
                .nodes()
                .get(owner)
                .ok_or_else(|| invalid("audio binding owner is missing"))?;
            if !matches!(node.kind, NodeKind::Source { .. } | NodeKind::Hold { .. })
                && !matches!(&node.kind, NodeKind::Retime { duration, mapping, pitch: PitchPolicy::Preserve, .. } if mapping.duration() != *duration)
            {
                return Err(invalid("audio binding owner is not a physical recipe"));
            }
            let duration = match &node.kind {
                NodeKind::Source { source } => source.duration,
                NodeKind::Hold { recipe } => recipe.duration,
                NodeKind::Retime { duration, .. } => *duration,
                _ => unreachable!("checked physical kind"),
            };
            if binding.resume.as_ref().is_some_and(|resume| {
                resume.local_boundary.compare_integer(0).is_lt()
                    || resume
                        .local_boundary
                        .compare_integer(duration.frames())
                        .is_gt()
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
            let terms = binding
                .resume
                .as_ref()
                .map_or(&[][..], |resume| resume.phase.terms.as_slice());
            for template in
                std::iter::once(&binding.lattice).chain(terms.iter().map(|term| &term.placement))
            {
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
        environment.validate()?;
        if &environment.instance().node != owner {
            return Err(invalid("audio binding occurrence names another owner"));
        }
        let binding = self
            .bindings
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
        let resume = binding
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
        for raw in raws.values() {
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
        Self::new(timings, bindings)
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
        let mut state = serializer.serialize_struct("AudioBindingState", 2)?;
        state.serialize_field("timings", &Timings(&self.timings))?;
        state.serialize_field("bindings", &self.bindings)?;
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
    }
    let mut template = |raw: &RawValue| -> Result<(), DocumentError> {
        let value: Template<'_> = serde_json::from_str(raw.get()).map_err(DocumentError::json)?;
        let args: Vec<&RawValue> =
            bounded_json_sequence(value.arguments.get(), MAX_DOCUMENT_DEPTH)?;
        let births: Vec<&RawValue> = bounded_json_sequence(value.births.get(), MAX_DOCUMENT_DEPTH)?;
        budget.spend(1 + args.len() + births.len())
    };
    let binding: Binding<'_> = serde_json::from_str(json).map_err(DocumentError::json)?;
    template(binding.lattice)?;
    if let Some(raw) = binding.resume {
        let resume: Resume<'_> = serde_json::from_str(raw.get()).map_err(DocumentError::json)?;
        let phase: Phase<'_> =
            serde_json::from_str(resume.phase.get()).map_err(DocumentError::json)?;
        let terms: Vec<&RawValue> =
            bounded_json_sequence(phase.terms.get(), MAX_AUDIO_BINDING_TERMS)?;
        for raw in terms {
            let term: Term<'_> = serde_json::from_str(raw.get()).map_err(DocumentError::json)?;
            template(term.placement)?;
        }
    }
    Ok(())
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
fn term_vec<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<AudioPhaseTerm>, D::Error> {
    BoundedSequence::<AudioPhaseTerm> {
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
