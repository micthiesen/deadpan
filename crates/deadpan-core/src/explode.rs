//! Convert one Repeat into an ordinary Sequence of independent plays.
//!
//! Specification §5.2: `explode` preserves every current picture, sound,
//! timing and override, remains undoable, and leaves no hidden live link
//! between the resulting copies. The Repeat keeps its identity, effects and
//! attachments and becomes the enclosing group. Each play becomes an owned
//! subtree at its unchanged absolute position: an existing override stays,
//! the first default play keeps the authored definition, and every other
//! default play receives a fresh copy that shares immutable media. Gaps become
//! explicit Holds and per-play escalation becomes an explicit group per play.
//!
//! Retained audio clocks are closed, never recomputed: a placement argument or
//! birth clause naming the exploded Repeat is resolved for its concrete play,
//! which yields exactly the clock the play already used.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::{
    AnchorLossPolicy, AudioBirthSurvivors, AudioClockRoot, AudioEdgePolicy, AudioPlacementTemplate,
    AudioRepeatValue, AudioTimingId, BeatNode, EditError, EditErrorCode, FrozenAudioKind,
    FrozenAudioLayout, IterationId, MAX_DOCUMENT_MARKS, MAX_DOCUMENT_NODES, Mark, MarkId,
    MarkLossReason, MarkState, NodeId, NodeKind, OccurrenceIdentities, ProjectDocument,
    RepeatInstance,
};

/// Fresh identities a host must supply for [`crate::Command::Explode`].
/// Nodes are consumed per play in play order: the copied definition in
/// structural preorder, then a materialized default gap, then an escalation
/// group. Marks are consumed per copied play in current mark ID order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ExplodeRequirements {
    pub nodes: usize,
    pub marks: usize,
    /// Whether a default gap is materialized, which needs the command's timing.
    pub captures_timing: bool,
}

struct Plan {
    child: NodeId,
    plays: Vec<IterationId>,
    /// The first play without an override keeps the authored definition.
    keep_default: Option<usize>,
    gap: Option<crate::HoldRecipe>,
    escalation: Option<crate::RepeatEscalation>,
    definition: Vec<NodeId>,
    owned_marks: usize,
}

impl Plan {
    fn new(document: &ProjectDocument, repeat: &NodeId) -> Result<Self, EditError> {
        let beat = document
            .nodes()
            .get(repeat)
            .ok_or_else(|| unavailable("explode target is missing"))?;
        let NodeKind::Repeat {
            child,
            iterations,
            gap,
            escalation,
        } = &beat.kind
        else {
            return Err(EditError::new(
                EditErrorCode::WrongNodeKind,
                "explode requires a Repeat",
            ));
        };
        let count = usize::try_from(iterations.len()).unwrap_or(usize::MAX);
        if count > MAX_DOCUMENT_NODES {
            return Err(limit("explode would exceed the document node limit"));
        }
        let plays: Vec<_> = (0..iterations.len())
            .map(|index| iterations.at(index).expect("play inside its order"))
            .collect();
        let overrides = document.overrides().get(repeat);
        let keep_default = plays
            .iter()
            .position(|play| overrides.is_none_or(|entries| entries.get(play).is_none()));
        let definition = crate::occurrence_edit::subtree_order(document, child)?;
        let members: BTreeSet<_> = definition.iter().collect();
        let owned_marks = document
            .marks()
            .values()
            .filter(|mark| {
                mark.bindings().any(|binding| {
                    members.contains(&binding.owner)
                        && matches!(
                            binding.coordinate,
                            crate::Anchor::Local { .. } | crate::Anchor::Source { .. }
                        )
                })
            })
            .count();
        Ok(Self {
            child: child.clone(),
            plays,
            keep_default,
            gap: gap.clone().filter(|gap| gap.duration.frames() > 0),
            escalation: *escalation,
            definition,
            owned_marks,
        })
    }

    fn copied(&self, document: &ProjectDocument, repeat: &NodeId, index: usize) -> bool {
        self.keep_default != Some(index)
            && document
                .overrides()
                .get(repeat)
                .is_none_or(|entries| entries.get(&self.plays[index]).is_none())
    }

    fn default_gap(&self, document: &ProjectDocument, repeat: &NodeId, index: usize) -> bool {
        self.gap.is_some()
            && index + 1 < self.plays.len()
            && document
                .gap_overrides()
                .get(repeat)
                .is_none_or(|entries| entries.get(&self.plays[index]).is_none())
    }

    fn escalated(
        &self,
        index: usize,
    ) -> Result<Option<(i32, Option<crate::FramingPose>)>, EditError> {
        let Some(escalation) = &self.escalation else {
            return Ok(None);
        };
        let play = u32::try_from(index).map_err(|_| limit("escalated play index"))?;
        let gain = i32::try_from(escalation.gain_millidecibels(play))
            .map_err(|_| invalid("repeat escalation gain is out of range"))?;
        let pose = escalation
            .pose(play)
            .map_err(|error| invalid(&error.to_string()))?;
        Ok((gain != 0 || pose.is_some()).then_some((gain, pose)))
    }

    fn requirements(
        &self,
        document: &ProjectDocument,
        repeat: &NodeId,
    ) -> Result<ExplodeRequirements, EditError> {
        let mut nodes = 0_usize;
        let mut marks = 0_usize;
        let mut captures_timing = false;
        for index in 0..self.plays.len() {
            if self.copied(document, repeat, index) {
                nodes = nodes
                    .checked_add(self.definition.len())
                    .ok_or_else(|| limit("explode identity count overflows"))?;
                marks = marks
                    .checked_add(self.owned_marks)
                    .ok_or_else(|| limit("explode identity count overflows"))?;
            }
            if self.default_gap(document, repeat, index) {
                captures_timing = true;
                nodes += 1;
            }
            if self.escalated(index)?.is_some() {
                nodes += 1;
            }
            if nodes > MAX_DOCUMENT_NODES {
                return Err(limit("explode would exceed the document node limit"));
            }
        }
        if document.nodes().len().saturating_add(nodes) > MAX_DOCUMENT_NODES {
            return Err(limit("explode would exceed the document node limit"));
        }
        if document.marks().len().saturating_add(marks) > MAX_DOCUMENT_MARKS {
            return Err(limit("explode would exceed the document mark limit"));
        }
        Ok(ExplodeRequirements {
            nodes,
            marks,
            captures_timing,
        })
    }
}

impl ProjectDocument {
    /// Exact fresh identity needs of exploding `repeat`. Read-only.
    pub fn explode_requirements(&self, repeat: &NodeId) -> Result<ExplodeRequirements, EditError> {
        let plan = Plan::new(self, repeat)?;
        refuse_unsupported(self, repeat)?;
        plan.requirements(self, repeat)
    }
}

/// Retained beat-sound journals prove their timing through an ordinary
/// Sequence scope; changing a Repeat inside such a scope is not yet admitted.
fn refuse_unsupported(document: &ProjectDocument, repeat: &NodeId) -> Result<(), EditError> {
    if document.audio_bindings().sound_clocks().is_empty() {
        return Ok(());
    }
    let scopes: BTreeSet<_> = document
        .audio_bindings()
        .sound_clocks()
        .values()
        .flat_map(|events| events.values().map(|journal| journal.scope()))
        .collect();
    let parents = parents(document);
    let mut node = repeat;
    for _ in 0..=crate::MAX_DOCUMENT_DEPTH {
        if scopes.contains(node) {
            return Err(invalid(
                "explode inside a retained beat sound scope is not yet supported",
            ));
        }
        match parents.get(node) {
            Some(parent) => node = parent,
            None => return Ok(()),
        }
    }
    Err(limit("explode ancestry exceeds the document depth limit"))
}

struct Pool<'a> {
    nodes: std::slice::Iter<'a, NodeId>,
    marks: std::slice::Iter<'a, MarkId>,
}

impl<'a> Pool<'a> {
    fn new(
        document: &ProjectDocument,
        supplied: &'a OccurrenceIdentities,
    ) -> Result<Self, EditError> {
        if supplied.nodes.len() > MAX_DOCUMENT_NODES || supplied.marks.len() > MAX_DOCUMENT_MARKS {
            return Err(invalid("explode identity pool exceeds document limits"));
        }
        // Retained audio names count as occupied, as in scoped isolation: a
        // copy's lineage and timing names must never alias a retired node.
        let mut nodes: BTreeSet<&NodeId> = document.nodes().keys().collect();
        nodes.extend(
            document
                .audio_lineage()
                .values()
                .map(|lineage| &lineage.origin),
        );
        for layout in document.audio_bindings().timings().values() {
            nodes.extend(layout.nodes().keys());
            nodes.extend(
                layout
                    .audio_lineage()
                    .values()
                    .map(|lineage| &lineage.origin),
            );
        }
        let mut marks = BTreeSet::new();
        if supplied.nodes.iter().any(|id| !nodes.insert(id))
            || supplied
                .marks
                .iter()
                .any(|id| document.marks().contains_key(id) || !marks.insert(id))
        {
            return Err(EditError::new(
                EditErrorCode::IdentityConflict,
                "explode identities must be fresh and distinct",
            ));
        }
        Ok(Self {
            nodes: supplied.nodes.iter(),
            marks: supplied.marks.iter(),
        })
    }
    fn node(&mut self) -> Result<NodeId, EditError> {
        self.nodes
            .next()
            .cloned()
            .ok_or_else(|| invalid("explode needs more node identities"))
    }
}

pub(crate) fn apply(
    document: &ProjectDocument,
    repeat: &NodeId,
    supplied: &OccurrenceIdentities,
    timing: &AudioTimingId,
    context: crate::command::EditContext<'_>,
) -> Result<ProjectDocument, EditError> {
    let allocation = context.allocation;
    let plan = Plan::new(document, repeat)?;
    refuse_unsupported(document, repeat)?;
    let requirements = plan.requirements(document, repeat)?;
    if requirements.captures_timing && &timing.allocation != allocation {
        return Err(invalid(
            "explode timing allocation must equal the new revision",
        ));
    }
    let mut pool = Pool::new(document, supplied)?;
    let edges = document.nodes()[repeat].audio_edges;
    let mut result = document.clone();
    if requirements.captures_timing {
        if document.audio_bindings().timings().contains_key(timing) {
            return Err(EditError::new(
                EditErrorCode::IdentityConflict,
                "explode timing identity already exists",
            ));
        }
        // Only the Repeat's own gap clock is needed. Other unbound owners keep
        // their implicit clocks, which explode leaves at the same positions.
        crate::gap_override::capture_gap_clock(&mut result, repeat, timing)?;
    }
    let mut allowances = context.allowances;
    let mut plays = Vec::with_capacity(plan.plays.len());
    let mut materialized = BTreeSet::new();
    for (index, iteration) in plan.plays.iter().enumerate() {
        let selected = RepeatInstance {
            node: repeat.clone(),
            iteration: iteration.clone(),
        };
        let root = if let Some(root) = result
            .overrides
            .get(repeat)
            .and_then(|entries| entries.get(iteration))
        {
            root.clone()
        } else if plan.keep_default == Some(index) {
            plan.child.clone()
        } else {
            let mut mapping = BTreeMap::new();
            for old in &plan.definition {
                mapping.insert(old.clone(), pool.node()?);
            }
            let marks = crate::marks::clone_occurrence_marks(&result, &selected, &mapping, || {
                pool.marks.next().cloned().ok_or_else(|| {
                    crate::DocumentError::new(
                        crate::DocumentErrorCode::InvalidIdentity,
                        "explode needs more mark identities",
                    )
                })
            })?;
            crate::occurrence_edit::clone_nodes(&mut result, &mapping, allocation)?;
            if let Some(allowances) = allowances.as_deref_mut() {
                allowances.explode_play(repeat, iteration, &mapping)?;
            }
            let copy = mapping[&plan.child].clone();
            result
                .overrides
                .entry(repeat.clone())
                .or_default()
                .insert(iteration.clone(), copy.clone());
            result.marks = marks;
            copy
        };
        let gap = if let Some(gap) = result
            .gap_overrides
            .get(repeat)
            .and_then(|entries| entries.get(iteration))
        {
            (index + 1 < plan.plays.len()).then(|| gap.clone())
        } else if plan.default_gap(document, repeat, index) {
            let id = pool.node()?;
            let recipe = plan.gap.as_ref().expect("default gap recipe");
            crate::gap_override::materialize(&mut result, repeat, iteration, &id, recipe, &edges)?;
            if let Some(allowances) = allowances.as_deref_mut() {
                allowances.explode_gap(repeat, iteration, &id)?;
            }
            materialized.insert(id.clone());
            Some(id)
        } else {
            None
        };
        let wrapper = match plan.escalated(index)? {
            Some(escalation) => Some((pool.node()?, escalation)),
            None => None,
        };
        plays.push((iteration.clone(), root, gap, wrapper));
    }

    // Close every retained clock that names this Repeat for its concrete play.
    for (iteration, root, gap, _) in &plays {
        for owned in std::iter::once(root).chain(gap) {
            for node in crate::occurrence_edit::subtree_order(&result, owned)? {
                close_owner(&mut result, &node, repeat, iteration)?;
            }
        }
    }
    result.audio_bindings.gap_bindings.remove(repeat);

    // Explicit gap branches previously carried the Repeat's gap boundaries.
    for (_, _, gap, _) in &plays {
        // A zero-duration branch had no gap extent, so no gap constraint.
        if let Some(gap) = gap
            && !materialized.contains(gap)
            && result.node_duration(gap)?.frames() > 0
        {
            let node = result.nodes.get_mut(gap).expect("gap branch root");
            node.audio_edges.node_start =
                hard_union(node.audio_edges.node_start, edges.repeat_gap_start);
            node.audio_edges.node_end = hard_union(node.audio_edges.node_end, edges.repeat_gap_end);
        }
    }

    // A reveal counts plays of the innermost enclosing Repeat. Captions that
    // counted this Repeat now either always show in their play or are absent.
    for (index, (_, root, gap, _)) in plays.iter().enumerate() {
        for owned in std::iter::once(root).chain(gap) {
            resolve_reveals(&mut result, owned, index)?;
        }
    }

    // Owned content that renders in no play disappears with the Repeat.
    let mut removed = BTreeSet::new();
    if plan.keep_default.is_none() {
        removed.extend(crate::occurrence_edit::subtree_order(&result, &plan.child)?);
    }
    if let Some((last, ..)) = plays.last()
        && let Some(dormant) = result
            .gap_overrides
            .get(repeat)
            .and_then(|entries| entries.get(last))
    {
        removed.extend(crate::occurrence_edit::subtree_order(&result, dormant)?);
    }

    let mut children = Vec::with_capacity(plays.len() * 2);
    for (_, root, gap, wrapper) in plays {
        let members: Vec<_> = std::iter::once(root).chain(gap).collect();
        match wrapper {
            Some((id, (gain, pose))) => {
                let mut group = BeatNode::sequence("Escalated play", members);
                if let Some(pose) = pose {
                    group.framing = Some(crate::Framing {
                        clock: Default::default(),
                        value: crate::FramingValue::Static { pose },
                    });
                }
                if gain != 0 {
                    let gain =
                        crate::GainDb::new(gain).map_err(|error| invalid(&error.to_string()))?;
                    let clip = crate::ClipGain::new(gain, false, Vec::new(), Vec::new())
                        .map_err(|error| invalid(&error.to_string()))?;
                    group.audio_treatments = crate::AudioTreatments::from_clip_gain(clip);
                }
                result.nodes.insert(id.clone(), group);
                children.push(id);
            }
            None => children.extend(members),
        }
    }
    for node in &removed {
        result.nodes.remove(node);
        result.beat_sounds.remove(node);
        result.overrides.remove(node);
        result.gap_overrides.remove(node);
        result.audio_lineage.remove(node);
        result.audio_bindings.bindings.remove(node);
        result.audio_bindings.gap_bindings.remove(node);
    }
    result.overrides.remove(repeat);
    result.gap_overrides.remove(repeat);
    let beat = result.nodes.get_mut(repeat).expect("explode target");
    beat.kind = NodeKind::Sequence { children };
    beat.audio_edges.repeat_gap_start = AudioEdgePolicy::default();
    beat.audio_edges.repeat_gap_end = AudioEdgePolicy::default();
    result.marks = finish_marks(&result.marks, repeat, &removed);
    if let Some(allowances) = allowances {
        allowances.strip_repeat(repeat)?;
    }
    result.validate_isolated_context()?;
    Ok(result)
}

fn hard_union(left: AudioEdgePolicy, right: AudioEdgePolicy) -> AudioEdgePolicy {
    if left == AudioEdgePolicy::Hard || right == AudioEdgePolicy::Hard {
        AudioEdgePolicy::Hard
    } else {
        left
    }
}

fn close_owner(
    result: &mut ProjectDocument,
    node: &NodeId,
    repeat: &NodeId,
    iteration: &IterationId,
) -> Result<(), EditError> {
    let timings = &result.audio_bindings.timings;
    for bindings in [
        &mut result.audio_bindings.bindings,
        &mut result.audio_bindings.gap_bindings,
    ] {
        let Some(binding) = bindings.get_mut(node) else {
            continue;
        };
        close(&mut binding.lattice, timings, repeat, iteration)?;
        if let Some(resume) = &mut binding.resume {
            for term in &mut resume.phase.terms {
                close(&mut term.placement, timings, repeat, iteration)?;
            }
        }
        for step in &mut binding.reanchors {
            let captured = step.placement.reference.root.clone();
            if close(&mut step.placement, timings, repeat, iteration)? && step.window.is_some() {
                // The exploded play was born on the Repeat's definition, which
                // already discarded a window authored in the enclosing captured
                // scope. An inner birth selecting that old captured root would
                // still have kept it, which a single root cannot express.
                if step.placement.births.iter().any(|clause| {
                    captured
                        == AudioClockRoot::DefinitionPointCeil {
                            root: clause.definition_root.clone(),
                        }
                }) {
                    return Err(invalid(
                        "explode cannot yet preserve this retained reanchor window",
                    ));
                }
                step.window = None;
            }
        }
    }
    Ok(())
}

/// Resolve one placement's dependence on `repeat` for `iteration`. Returns
/// true when the clock root changes to the Repeat's definition birth.
fn close(
    template: &mut AudioPlacementTemplate,
    timings: &BTreeMap<AudioTimingId, FrozenAudioLayout>,
    repeat: &NodeId,
    iteration: &IterationId,
) -> Result<bool, EditError> {
    if matches!(&template.gap_after, Some(AudioRepeatValue::Live { repeat: live }) if live == repeat)
    {
        return Err(invalid(
            "explode found an unmaterialized gap clock for its Repeat",
        ));
    }
    let clause = template
        .births
        .iter()
        .position(|clause| &clause.repeat == repeat);
    let argument = template.arguments.iter().position(
        |argument| matches!(&argument.value, AudioRepeatValue::Live { repeat: live } if live == repeat),
    );
    if clause.is_none() && argument.is_none() {
        return Ok(false);
    }
    let layout = timings
        .get(&template.reference.timing)
        .ok_or_else(|| invalid("audio timing identity is missing"))?;
    let born = match clause {
        None => false,
        Some(index) => !survives(&template.births[index].survivors, layout, iteration)?,
    };
    if !born {
        if let Some(index) = argument {
            template.arguments[index].value = AudioRepeatValue::Captured {
                iteration: iteration.clone(),
            };
        }
        if let Some(index) = clause {
            template.births.remove(index);
        }
        return Ok(false);
    }
    let index = clause.expect("born through a clause");
    let definition = template.births[index].definition_root.clone();
    let (required, _) = layout
        .scoped_repeats(
            &definition,
            &template.reference.physical,
            MAX_DOCUMENT_NODES,
        )
        .map_err(EditError::from)?;
    template
        .arguments
        .retain(|argument| required.contains(&argument.reference_repeat));
    template.births.drain(..=index);
    let root = AudioClockRoot::DefinitionPointCeil { root: definition };
    let changed = template.reference.root != root;
    template.reference.root = root;
    Ok(changed)
}

fn survives(
    survivors: &AudioBirthSurvivors,
    layout: &FrozenAudioLayout,
    iteration: &IterationId,
) -> Result<bool, EditError> {
    Ok(match survivors {
        AudioBirthSurvivors::CapturedRepeat { repeat } => {
            let Some(crate::FrozenAudioNode {
                kind: FrozenAudioKind::Repeat { iterations, .. },
                ..
            }) = layout.nodes().get(repeat)
            else {
                return Err(invalid("birth survivors require a captured Repeat"));
            };
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
    })
}

/// Captions on nodes whose innermost enclosing Repeat is the exploded one.
fn resolve_reveals(
    result: &mut ProjectDocument,
    root: &NodeId,
    play: usize,
) -> Result<(), EditError> {
    let position = u32::try_from(play)
        .ok()
        .and_then(|play| play.checked_add(1))
        .ok_or_else(|| limit("revealed play position"))?;
    let mut pending = vec![root.clone()];
    let mut visited = 0_usize;
    while let Some(node) = pending.pop() {
        visited += 1;
        if visited > MAX_DOCUMENT_NODES {
            return Err(limit("explode caption traversal"));
        }
        let beat = result.nodes.get_mut(&node).expect("owned play node");
        if beat.captions.iter().any(|caption| caption.reveal.is_some()) {
            beat.captions.retain_mut(|caption| match caption.reveal {
                Some(reveal) if reveal.get() > position => false,
                Some(_) => {
                    caption.reveal = None;
                    true
                }
                None => true,
            });
        }
        if !matches!(beat.kind, NodeKind::Repeat { .. }) {
            pending.extend(result.children(&node).cloned());
        }
    }
    Ok(())
}

fn finish_marks(
    marks: &BTreeMap<MarkId, Mark>,
    repeat: &NodeId,
    removed: &BTreeSet<NodeId>,
) -> BTreeMap<MarkId, Mark> {
    let mut output = BTreeMap::new();
    for (id, mark) in marks {
        let mut bindings = Vec::with_capacity(mark.binding_count());
        for original in mark.bindings() {
            if matches!(original.state, MarkState::Unresolved { .. }) {
                bindings.push(original);
                continue;
            }
            let mut binding = original.clone();
            let lost = if removed.contains(&binding.owner) {
                Some(MarkLossReason::OwnerMissing)
            } else {
                match &mut binding.coordinate {
                    crate::Anchor::Local { node, .. } if removed.contains(node) => {
                        Some(MarkLossReason::HostMissing)
                    }
                    crate::Anchor::Occurrence { instance, .. } => {
                        if removed.contains(&instance.node) {
                            Some(MarkLossReason::OccurrenceMissing)
                        } else {
                            instance.repeats.retain(|step| &step.node != repeat);
                            None
                        }
                    }
                    _ => None,
                }
            };
            match lost {
                None => bindings.push(binding),
                Some(reason) if mark.loss_policy == AnchorLossPolicy::KeepUnresolved => {
                    let mut unresolved = original;
                    unresolved.state = MarkState::Unresolved { reason };
                    bindings.push(unresolved);
                }
                Some(_) => {}
            }
        }
        if let Some(mark) = mark.with_bindings(bindings) {
            output.insert(id.clone(), mark);
        }
    }
    output
}

fn parents(document: &ProjectDocument) -> BTreeMap<&NodeId, &NodeId> {
    document
        .nodes()
        .keys()
        .flat_map(|id| document.children(id).map(move |child| (child, id)))
        .collect()
}

fn invalid(message: &str) -> EditError {
    EditError::new(EditErrorCode::InvalidCommand, message)
}

fn unavailable(message: &str) -> EditError {
    EditError::new(EditErrorCode::SelectionUnavailable, message)
}

fn limit(message: &str) -> EditError {
    EditError::new(EditErrorCode::LimitExceeded, message)
}
