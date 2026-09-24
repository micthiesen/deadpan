//! A pure cut changes allocation while retaining each side's complete context.
//! Caller-supplied identities and physical mark bindings make the cut reversible.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    Anchor, AnchorIndex, BeatNode, DocumentError, EditError, EditErrorCode, ExactRatio,
    FrameDuration, FrameRange, InsertionBias, InstancePath, MAX_DOCUMENT_MARK_BINDINGS,
    MAX_DOCUMENT_NODES, MAX_MARK_BINDINGS, MarkFragment, MarkState, NodeId, NodeKind, PitchPolicy,
    ProjectDocument, ProjectFrame, RetimePurpose, RevisionId,
};

/// Fresh node IDs, consumed deterministically. Unused IDs are not persisted.
/// A new pair consumes left/right partition IDs, then the right subtree in
/// preorder; root cuts additionally consume a left context ID before that copy.
/// Refinement consumes the right partition ID and its full child copy. A needed
/// non-Sequence parent container is last in either case.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SplitIdentities {
    pub nodes: Vec<NodeId>,
}

pub(crate) fn apply(
    document: &ProjectDocument,
    target: &NodeId,
    at: FrameDuration,
    identities: &SplitIdentities,
    allocation: &RevisionId,
) -> Result<ProjectDocument, EditError> {
    let durations = document.durations()?;
    let original = document.nodes.get(target).ok_or_else(|| {
        EditError::new(
            EditErrorCode::SelectionUnavailable,
            "split target is missing",
        )
    })?;
    let duration = durations[target];
    if at.frames() <= 0 || at >= duration {
        return Err(invalid("split must be strictly inside the selected beat"));
    }
    if identities.nodes.len() > MAX_DOCUMENT_NODES {
        return Err(limit("split identity pool exceeds the document node limit"));
    }
    let mut distinct = BTreeSet::new();
    if identities
        .nodes
        .iter()
        .any(|id| document.nodes.contains_key(id) || !distinct.insert(id))
    {
        return Err(EditError::new(
            EditErrorCode::IdentityConflict,
            "split identities must be fresh and distinct",
        ));
    }
    let index = AnchorIndex::from_durations(document, durations)?;
    let root = target == document.root();
    let parent = index.parents.get(target).map(|(parent, _)| parent);
    let container = parent
        .is_some_and(|parent| !matches!(document.nodes[parent].kind, NodeKind::Sequence { .. }));
    let refinement = match &original.kind {
        NodeKind::Retime {
            child,
            mapping,
            purpose: RetimePurpose::Partition,
            ..
        } if original.framing.is_none() => Some((child, *mapping)),
        _ => None,
    };
    let context = refinement.map_or(target, |(child, _)| child);
    let order = crate::occurrence_edit::subtree_order(document, context)?;
    let required = order.len()
        + if refinement.is_some() { 1 } else { 2 }
        + usize::from(root)
        + usize::from(container);
    if document
        .nodes
        .len()
        .checked_add(required)
        .is_none_or(|n| n > MAX_DOCUMENT_NODES)
    {
        return Err(limit("split exceeds the document node limit"));
    }
    if identities.nodes.len() < required {
        return Err(invalid("split needs more node identities"));
    }
    let mut supplied = identities.nodes.iter();
    // The size preflight makes these bounded identity allocations infallible.
    let left = if refinement.is_some() {
        target.clone()
    } else {
        supplied.next().expect("preflighted partition").clone()
    };
    let right = supplied.next().expect("preflighted partition").clone();
    let left_context = if root {
        supplied.next().expect("preflighted root context").clone()
    } else {
        context.clone()
    };
    let mut copied = BTreeMap::new();
    for old in order {
        copied.insert(old, supplied.next().expect("preflighted subtree").clone());
    }
    let mut right_bindings = copied.clone();
    if refinement.is_some() {
        right_bindings.insert(target.clone(), right.clone());
    }
    // The root is a surviving full-duration scope and owner, unlike its copied
    // context. Only bindings to descendants gain a second physical host.
    if root {
        right_bindings.remove(target);
    }
    let marks = split_marks(
        document,
        &index,
        target,
        at,
        refinement.is_some(),
        &right_bindings,
    )?;
    let mut result = document.clone();
    crate::occurrence_edit::clone_nodes(&mut result, &copied, allocation)?;
    if root {
        result.nodes.insert(left_context.clone(), original.clone());
        let lineage = result.audio_lineage[target].clone();
        result.audio_lineage.insert(left_context.clone(), lineage);
    }
    let start = refinement.map_or(0, |(_, mapping)| mapping.start().0);
    let seam = start
        .checked_add(at.frames())
        .ok_or_else(|| invalid("split time overflow"))?;
    let end = start
        .checked_add(duration.frames())
        .ok_or_else(|| invalid("split time overflow"))?;
    let pitch = match &original.kind {
        NodeKind::Retime {
            pitch,
            purpose: RetimePurpose::Partition,
            ..
        } => *pitch,
        _ => PitchPolicy::Preserve,
    };
    result.nodes.insert(
        left.clone(),
        partition(&original.label, left_context, start, seam, pitch)?,
    );
    result.nodes.insert(
        right.clone(),
        partition(&original.label, copied[context].clone(), seam, end, pitch)?,
    );
    if root {
        result.nodes.insert(
            target.clone(),
            BeatNode::sequence(&original.label, vec![left, right]),
        );
    } else if container {
        let wrapper = supplied
            .next()
            .expect("preflighted parent container")
            .clone();
        result.nodes.insert(
            wrapper.clone(),
            BeatNode::sequence(&original.label, vec![left, right]),
        );
        crate::command::replace_child(
            &mut result,
            parent.expect("non-root parent"),
            target,
            wrapper,
        )?;
    } else {
        let NodeKind::Sequence { children } = &mut result
            .nodes
            .get_mut(parent.expect("non-root parent"))
            .expect("indexed parent")
            .kind
        else {
            return Err(invalid("split parent is not a Sequence"));
        };
        let slot = children
            .iter()
            .position(|child| child == target)
            .ok_or_else(|| invalid("split target is not a child of its parent"))?;
        children.splice(slot..=slot, [left, right]);
    }
    result.marks = marks;
    result.validate()?;
    Ok(result)
}

fn partition(
    label: &str,
    child: NodeId,
    start: i64,
    end: i64,
    pitch: PitchPolicy,
) -> Result<BeatNode, EditError> {
    let mapping =
        FrameRange::new(ProjectFrame(start), ProjectFrame(end)).map_err(DocumentError::from)?;
    Ok(BeatNode {
        framing: None,
        label: label.into(),
        kind: NodeKind::Retime {
            child,
            duration: mapping.duration(),
            mapping,
            pitch,
            purpose: RetimePurpose::Partition,
        },
        audio_edges: Default::default(),
    })
}

fn split_marks(
    document: &ProjectDocument,
    index: &AnchorIndex<'_>,
    target: &NodeId,
    at: FrameDuration,
    refinement: bool,
    mapping: &BTreeMap<NodeId, NodeId>,
) -> Result<BTreeMap<crate::MarkId, crate::Mark>, EditError> {
    let mut marks = BTreeMap::new();
    let mut total = 0usize;
    for (id, mark) in document.marks() {
        let mut bindings = Vec::new();
        for binding in mark.bindings() {
            let selected = if binding.state != MarkState::Bound {
                None
            } else {
                match &binding.coordinate {
                    Anchor::Occurrence { instance, position }
                        if mapping.contains_key(&instance.node) =>
                    {
                        Some(relative_position(index, instance, *position, target)?)
                    }
                    Anchor::Local { node, position } if refinement && node == target => {
                        Some(*position)
                    }
                    _ => None,
                }
            };
            if let Some(position) = selected {
                // Concrete events move once. A retained context can be hidden by
                // an outer partition's bias, so choose in the old target domain.
                let right = position.compare_integer(at.frames()).is_gt()
                    || (position.compare_integer(at.frames()).is_eq()
                        && mark.boundary.bias == InsertionBias::Right);
                let remapped = if right {
                    remap_binding(&binding, mapping, refinement.then_some((target, at)))?
                } else {
                    binding
                };
                push_binding(&mut bindings, remapped)?;
            } else {
                push_binding(&mut bindings, binding.clone())?;
                push_binding(&mut bindings, remap_binding(&binding, mapping, None)?)?;
            }
        }
        total = total
            .checked_add(bindings.len())
            .ok_or_else(|| limit("split mark binding count overflow"))?;
        if total > MAX_DOCUMENT_MARK_BINDINGS {
            return Err(limit("split exceeds the total document mark binding limit"));
        }
        if let Some(mark) = mark.with_bindings(bindings) {
            marks.insert(id.clone(), mark);
        }
    }
    Ok(marks)
}

fn push_binding(bindings: &mut Vec<MarkFragment>, binding: MarkFragment) -> Result<(), EditError> {
    if !bindings.contains(&binding) {
        if bindings.len() >= MAX_MARK_BINDINGS {
            return Err(limit("split exceeds the per-mark binding limit"));
        }
        bindings.push(binding);
    }
    Ok(())
}

fn remap_binding(
    binding: &MarkFragment,
    mapping: &BTreeMap<NodeId, NodeId>,
    translated: Option<(&NodeId, FrameDuration)>,
) -> Result<MarkFragment, EditError> {
    let mut result = binding.clone();
    if let Some(owner) = mapping.get(&result.owner) {
        result.owner = owner.clone();
    }
    // Unresolved coordinates record the last authored address, never a new one.
    if result.state != MarkState::Bound {
        return Ok(result);
    }
    match &mut result.coordinate {
        Anchor::Local { node, position } => {
            translate(position, node, translated)?;
            if let Some(new) = mapping.get(node) {
                *node = new.clone();
            }
        }
        Anchor::Occurrence { instance, position } => {
            translate(position, &instance.node, translated)?;
            crate::occurrence_edit::remap_instance(instance, mapping);
        }
        Anchor::Source { .. } | Anchor::Sequence { .. } => {}
    }
    Ok(result)
}

fn translate(
    position: &mut ExactRatio,
    host: &NodeId,
    translated: Option<(&NodeId, FrameDuration)>,
) -> Result<(), EditError> {
    if let Some((_, at)) = translated.filter(|(target, _)| *target == host) {
        *position = position
            .checked_sub(ExactRatio::integer(at.frames()))
            .map_err(DocumentError::from)?;
    }
    Ok(())
}

/// Map only as far as the selected output. The old document already validated
/// the complete concrete path, including effective sparse-override children.
fn relative_position(
    index: &AnchorIndex<'_>,
    instance: &InstancePath,
    mut position: ExactRatio,
    target: &NodeId,
) -> Result<ExactRatio, EditError> {
    let mut node = &instance.node;
    while node != target {
        let (parent, offset) = index
            .parents
            .get(node)
            .ok_or_else(|| invalid("split occurrence is outside target"))?;
        position = match &index.document.nodes[parent].kind {
            NodeKind::Sequence { .. } => position.checked_add(ExactRatio::integer(*offset)),
            NodeKind::Repeat { .. } => {
                let step = instance
                    .repeats
                    .iter()
                    .find(|step| &step.node == parent)
                    .ok_or_else(|| invalid("split occurrence is missing its repeat identity"))?;
                let play = index.repeats[parent]
                    .play(&step.iteration)
                    .ok_or_else(|| invalid("split occurrence play is missing"))?;
                position.checked_add(ExactRatio::integer(play.start))
            }
            NodeKind::Retime {
                mapping, duration, ..
            } => position
                .checked_sub(ExactRatio::integer(mapping.start().0))
                .and_then(|position| {
                    position.checked_mul(ExactRatio::new(
                        i128::from(duration.frames()),
                        i128::from(mapping.duration().frames()),
                    )?)
                }),
            _ => return Err(invalid("split occurrence has a leaf parent")),
        }
        .map_err(DocumentError::from)?;
        node = parent;
    }
    Ok(position)
}

fn invalid(message: &str) -> EditError {
    EditError::new(EditErrorCode::InvalidCommand, message)
}
fn limit(message: &str) -> EditError {
    EditError::new(EditErrorCode::LimitExceeded, message)
}
