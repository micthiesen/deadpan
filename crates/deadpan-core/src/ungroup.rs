//! Ungroup removes an ordinary Sequence and moves the group's own picture and
//! sound behavior onto its promoted children so nothing they render changes.
//!
//! Framing composes provider to root, so a child without framing takes the
//! group's camera path windowed to its own interval of the same path. Gain is
//! applied per contribution in the shared Q32 dB sum, so the group's clip gain
//! translated into each child's clock adds exactly; a child whose own recipe is
//! clip gain alone absorbs it. Edge choices belong to the group's outer
//! boundaries and move to the first and last children with positive duration.
//! When a child cannot absorb these exactly (its own framing, other
//! treatments, or a Partition that cannot carry the edge), it gets a new
//! single-child Sequence carrying the group's windowed behavior instead.
//! Stateful or nonlinear stages (EQ, saturation, balance) and sends shape the
//! group's mixed children together, so no per-child form is sample-identical:
//! those groups refuse.

use crate::{
    AudioEdgePolicies, AudioEdgePolicy, AudioEditorialEdges, AudioTreatmentStage, AudioTreatments,
    BeatNode, ClipGain, DocumentError, EditError, EditErrorCode, ExactRatio, Framing, FramingClock,
    NodeId, NodeKind, ProjectDocument, RetimePurpose,
};

/// What one promoted child carries after the group is removed.
struct Promotion {
    child: NodeId,
    /// The child with the group's behavior absorbed, or the wrapper's
    /// recipe when it cannot absorb it exactly.
    absorbed: Option<BeatNode>,
    wrapper: Option<BeatNode>,
}

pub(crate) struct UngroupPlan {
    parent: NodeId,
    children: Vec<NodeId>,
    promotions: Vec<Promotion>,
}

impl UngroupPlan {
    pub(crate) fn wrappers(&self) -> usize {
        self.promotions
            .iter()
            .filter(|promotion| promotion.wrapper.is_some())
            .count()
    }
}

fn invalid(message: &'static str) -> EditError {
    EditError::new(EditErrorCode::InvalidCommand, message)
}

impl ProjectDocument {
    /// Fresh node identities `Command::Ungroup` needs for per-child wrappers.
    pub fn ungroup_requirements(&self, node: &NodeId) -> Result<usize, EditError> {
        plan(self, node).map(|plan| plan.wrappers())
    }
}

pub(crate) fn plan(document: &ProjectDocument, node: &NodeId) -> Result<UngroupPlan, EditError> {
    let parent = document.parent_of(node).ok_or_else(|| {
        EditError::new(
            EditErrorCode::SelectionUnavailable,
            "operation requires an existing non-root node",
        )
    })?;
    if !matches!(
        document.nodes.get(&parent).map(|node| &node.kind),
        Some(NodeKind::Sequence { .. })
    ) {
        return Err(EditError::new(
            EditErrorCode::WrongNodeKind,
            "operation requires a Sequence child; select its enclosing Repeat or Retime to remove that structure",
        ));
    }
    let beat = document.nodes.get(node).ok_or_else(|| {
        EditError::new(
            EditErrorCode::SelectionUnavailable,
            format!("node {node} does not exist"),
        )
    })?;
    let NodeKind::Sequence { children } = &beat.kind else {
        return Err(EditError::new(
            EditErrorCode::WrongNodeKind,
            "ungroup requires a Sequence",
        ));
    };
    if !beat.cutaways.is_empty() || !beat.captions.is_empty() {
        return Err(invalid(
            "ungroup cannot move this Sequence's cutaways or captions; retain the group",
        ));
    }
    let gain = group_gain(&beat.audio_treatments)?;
    let edges = (beat.audio_editorial_edges, beat.audio_edges);
    let has_edges = !edges.0.is_empty() || edges.1 != AudioEdgePolicies::default();
    if beat.framing.is_none() && gain.is_none() && !has_edges {
        return Ok(UngroupPlan {
            parent,
            children: children.clone(),
            promotions: Vec::new(),
        });
    }
    let extents = document.exact_durations()?;
    let positive: Vec<_> = children
        .iter()
        .filter(|child| extents[*child].compare_integer(0).is_gt())
        .collect();
    if has_edges && positive.is_empty() {
        // An empty group's edge choice applies at a seam between its
        // neighbors, which no child can carry.
        return Err(invalid(
            "ungroup cannot move this empty Sequence's audio edge choices; reset them or retain the group",
        ));
    }
    let (first, last) = (positive.first().copied(), positive.last().copied());
    let mut offset = ExactRatio::ZERO;
    let mut promotions = Vec::new();
    for child in children {
        let duration = extents[child];
        let framing = beat
            .framing
            .as_ref()
            .map(|framing| windowed_framing(framing, offset, extents[node]))
            .transpose()?;
        let clip = gain
            .map(|gain| gain.windowed(offset, duration))
            .transpose()
            .map_err(|error| EditError::new(EditErrorCode::InvalidCommand, error.to_string()))?
            .filter(|clip| *clip != ClipGain::default());
        let mut editorial = AudioEditorialEdges::default();
        let mut policies = AudioEdgePolicies::default();
        if Some(child) == first {
            editorial.start = edges.0.start;
            policies.node_start = edges.1.node_start;
        }
        if Some(child) == last {
            editorial.end = edges.0.end;
            policies.node_end = edges.1.node_end;
        }
        offset = offset.checked_add(duration).map_err(DocumentError::from)?;
        if framing.is_none()
            && clip.is_none()
            && editorial.is_empty()
            && policies == AudioEdgePolicies::default()
        {
            continue;
        }
        let current = &document.nodes[child];
        let absorbed = absorb(
            current,
            framing.as_ref(),
            clip.as_ref(),
            editorial,
            policies,
        );
        let wrapper = absorbed.is_none().then(|| {
            let mut wrapper = BeatNode::sequence(beat.label.clone(), vec![child.clone()]);
            wrapper.framing = framing;
            if let Some(clip) = clip {
                wrapper.audio_treatments = AudioTreatments::from_clip_gain(clip);
            }
            wrapper.audio_editorial_edges = editorial;
            wrapper.audio_edges = policies;
            wrapper
        });
        promotions.push(Promotion {
            child: child.clone(),
            absorbed,
            wrapper,
        });
    }
    Ok(UngroupPlan {
        parent,
        children: children.clone(),
        promotions,
    })
}

/// The group's clip gain, or a readable refusal for stages that act on the
/// group's mixed children together.
fn group_gain(treatments: &AudioTreatments) -> Result<Option<&ClipGain>, EditError> {
    if !treatments.tail_sends().is_empty() || !treatments.wet_sends().is_empty() {
        return Err(invalid(
            "ungroup cannot split this Sequence's effect sends across its children; retain the group",
        ));
    }
    for stage in treatments.order() {
        match stage {
            AudioTreatmentStage::ClipGain => {}
            AudioTreatmentStage::Saturation => {
                return Err(invalid(
                    "ungroup cannot preserve this Sequence's saturation, which shapes its children's mixed sound together; retain the group",
                ));
            }
            AudioTreatmentStage::Equalizer => {
                return Err(invalid(
                    "ungroup cannot preserve this Sequence's EQ, which filters its children's sound continuously across their cuts; retain the group",
                ));
            }
            AudioTreatmentStage::StereoBalance => {
                return Err(invalid(
                    "ungroup cannot preserve this Sequence's stereo balance, which applies to its children's mixed sound; retain the group",
                ));
            }
        }
    }
    Ok(treatments.clip_gain())
}

/// The same camera path, read through the child's interval of the group clock.
fn windowed_framing(
    framing: &Framing,
    offset: ExactRatio,
    group: ExactRatio,
) -> Result<Framing, EditError> {
    let (base, duration) = match framing.clock {
        FramingClock::OwnerOutput => (ExactRatio::ZERO, group),
        FramingClock::RetainedOutput { offset, duration } => (offset, duration),
    };
    let mut moved = framing.clone();
    moved.clock = FramingClock::RetainedOutput {
        offset: base.checked_add(offset).map_err(DocumentError::from)?,
        duration,
    };
    moved
        .validate()
        .map_err(|error| EditError::new(EditErrorCode::InvalidCommand, error.to_string()))?;
    Ok(moved)
}

/// The child carrying the group's behavior itself, when that is exact.
fn absorb(
    child: &BeatNode,
    framing: Option<&Framing>,
    clip: Option<&ClipGain>,
    editorial: AudioEditorialEdges,
    policies: AudioEdgePolicies,
) -> Option<BeatNode> {
    let mut next = child.clone();
    if let Some(framing) = framing {
        if child.framing.is_some() {
            return None;
        }
        next.framing = Some(framing.clone());
    }
    if let Some(clip) = clip {
        let own = &child.audio_treatments;
        next.audio_treatments = if own.is_empty() {
            AudioTreatments::from_clip_gain(clip.clone())
        } else if own.order() == [AudioTreatmentStage::ClipGain]
            && own.tail_sends().is_empty()
            && own.wet_sends().is_empty()
        {
            // Owners evaluate innermost first; with no other stage between
            // them, the child's and group's gains are one exact sum.
            AudioTreatments::from_clip_gain(own.clip_gain()?.followed_by(clip)?)
        } else {
            return None;
        };
    }
    // Several coincident owners produce one fade; any Hard suppresses it.
    next.audio_editorial_edges.start |= editorial.start;
    next.audio_editorial_edges.end |= editorial.end;
    for (side, policy) in [
        (&mut next.audio_edges.node_start, policies.node_start),
        (&mut next.audio_edges.node_end, policies.node_end),
    ] {
        if policy == AudioEdgePolicy::Hard {
            *side = AudioEdgePolicy::Hard;
        }
    }
    if matches!(
        next.kind,
        NodeKind::Retime {
            purpose: RetimePurpose::Partition,
            ..
        }
    ) && !next
        .audio_editorial_edges
        .permits_partition_policies(next.audio_edges)
    {
        return None;
    }
    Some(next)
}

/// Replace the group by its children, installing absorbed recipes and
/// wrappers with the caller's exact fresh identities.
pub(crate) fn apply(
    document: &mut ProjectDocument,
    node: &NodeId,
    wrappers: &[NodeId],
) -> Result<(), EditError> {
    let plan = plan(document, node)?;
    if wrappers.len() != plan.wrappers() {
        return Err(EditError::new(
            EditErrorCode::IdentityConflict,
            format!(
                "this ungroup requires exactly {} wrapper identities",
                plan.wrappers()
            ),
        ));
    }
    let mut promoted = plan.children.clone();
    let mut fresh = wrappers.iter();
    for promotion in plan.promotions {
        if let Some(mut wrapper) = promotion.wrapper {
            let id = fresh.next().expect("wrapper count was checked").clone();
            if document.nodes.contains_key(&id) || id == *node {
                return Err(EditError::new(
                    EditErrorCode::IdentityConflict,
                    format!("node {id} already exists"),
                ));
            }
            wrapper.kind = NodeKind::Sequence {
                children: vec![promotion.child.clone()],
            };
            for slot in promoted.iter_mut().filter(|slot| **slot == promotion.child) {
                *slot = id.clone();
            }
            document.nodes.insert(id, wrapper);
        } else if let Some(absorbed) = promotion.absorbed {
            document.nodes.insert(promotion.child, absorbed);
        }
    }
    let NodeKind::Sequence { children } = &mut document
        .nodes
        .get_mut(&plan.parent)
        .expect("planned parent exists")
        .kind
    else {
        unreachable!("planned parent is a Sequence")
    };
    let position = children
        .iter()
        .position(|id| id == node)
        .expect("planned group is a child of its parent");
    children.splice(position..=position, promoted);
    document.nodes.remove(node);
    Ok(())
}
