//! Timing edits of one already isolated Repeat occurrence. Every physical
//! owner after the change keeps its own pre-edit clock: later siblings, later
//! plays and gaps of each enclosing Repeat, and everything after them. Shared
//! owners keep their earlier occurrences unchanged because the capture window
//! starts at the changed boundary.

use crate::insert_time::composite;
use crate::{
    Anchor, AnchorIndex, AnchorTarget, AudioTimingId, BeatNode, BoundaryAnchor, Command,
    DocumentError, EditError, EditErrorCode, ExactRatio, FrameDuration, InsertionBias,
    InstancePath, IterationOrder, NodeId, NodeKind, ProjectDocument, ProjectFrame,
    WrapAnchorPolicy,
};

/// The occurrence's Sequence parent, its slot and its exact root interval.
pub(crate) struct Site {
    pub parent: NodeId,
    pub slot: usize,
    pub start: i64,
    pub end: i64,
}

/// Admit a beat directly inside an ordinary group of the selected play, with
/// only Sequence and Repeat ancestors, so the root interval is integral.
pub(crate) fn site(document: &ProjectDocument, instance: &InstancePath) -> Result<Site, EditError> {
    instance.validate(document)?;
    let durations = document.durations()?;
    let index = AnchorIndex::from_durations(document, durations.clone())?;
    let (parent, _) = index
        .parents
        .get(&instance.node)
        .cloned()
        .ok_or_else(|| unavailable("a one-play timing edit needs a beat inside the play"))?;
    let NodeKind::Sequence { children } = &document.nodes()[&parent].kind else {
        return Err(unavailable(
            "this beat is the play's whole contents; split it first, or press Enter to edit a beat in its group",
        ));
    };
    let slot = children
        .iter()
        .position(|child| child == &instance.node)
        .ok_or_else(|| invalid("the beat is not a child of its group"))?;
    let mut ancestor = Some(&parent);
    while let Some(node) = ancestor {
        if matches!(document.nodes()[node].kind, NodeKind::Retime { .. }) {
            return Err(invalid(
                "a one-play deletion or Repeat inside a speed change is not supported yet",
            ));
        }
        ancestor = index.parents.get(node).map(|(parent, _)| parent);
    }
    let start = index
        .resolve_target(&AnchorTarget {
            boundary: BoundaryAnchor {
                coordinate: Anchor::Occurrence {
                    instance: instance.clone(),
                    position: ExactRatio::ZERO,
                },
                bias: InsertionBias::Right,
            },
            occurrence: None,
        })
        .map_err(|error| invalid(&error.to_string()))?;
    if start.exact_frame.denominator() != 1 {
        return Err(invalid(
            "a one-play timing edit needs an integral play clock",
        ));
    }
    let start = start.frame.0;
    let end = start
        .checked_add(durations[&instance.node].frames())
        .ok_or_else(overflow)?;
    Ok(Site {
        parent,
        slot,
        start,
        end,
    })
}

/// Remove the beat from its play only; the rest of that play, later plays and
/// everything after the Repeat move earlier on their own retained clocks.
pub(crate) fn delete(
    document: &ProjectDocument,
    instance: &InstancePath,
    timing: &AudioTimingId,
    allocation: &crate::RevisionId,
) -> Result<ProjectDocument, EditError> {
    validate_timing(document, timing, allocation)?;
    let site = site(document, instance)?;
    let total = document.duration()?.frames();
    let working = if site.start == site.end || site.end == total {
        // No surviving time moves. Avoid retaining an unused sampling layout.
        document.clone()
    } else {
        let affected = composite::occurrence_shifted_owners(document, &site.parent, site.slot + 1)?;
        composite::prepare_owners(document, &affected, ProjectFrame(site.end), total, timing)?
    };
    let mut result = working.clone();
    crate::command::reduce(
        &mut result,
        &Command::Delete {
            node: instance.node.clone(),
        },
        allocation,
    )?;
    let command = Command::DeleteRipple {
        node: instance.node.clone(),
        timing: timing.clone(),
    };
    crate::audio_lineage::reconcile(&working, &mut result, &command)?;
    result.marks = crate::marks::transform_marks(&working, &result, &command)?;
    crate::audio_binding_lifecycle::prune(&mut result);
    result.validate()?;
    Ok(result)
}

/// Wrap the beat in a new Repeat in its play only. Its first play keeps the
/// beat's old sample phase; everything after the beat keeps its own clock.
pub(crate) fn wrap_repeat(
    original: &ProjectDocument,
    document: &ProjectDocument,
    instance: &InstancePath,
    id: &NodeId,
    plays: u32,
    timing: &AudioTimingId,
    allocation: &crate::RevisionId,
) -> Result<ProjectDocument, EditError> {
    if plays == 0 {
        return Err(invalid("Repeat total plays must be positive"));
    }
    validate_timing(document, timing, allocation)?;
    // The new play identities use this revision's allocation, as for an
    // ordinary Repeat; isolation copies only reuse existing play orders.
    if original.nodes().values().any(|node| {
        matches!(&node.kind, NodeKind::Repeat { iterations, .. }
            if iterations.segments().any(|(existing, _, _)| existing == allocation))
    }) {
        return Err(EditError::new(
            EditErrorCode::IdentityConflict,
            "Repeat reuses an existing allocation",
        ));
    }
    if crate::scoped_edit::occupied_node_ids(document).contains(id) {
        return Err(EditError::new(
            EditErrorCode::IdentityConflict,
            "the new Repeat identity must be fresh, including retained audio names",
        ));
    }
    if !document.sound_allowances.is_empty() {
        return Err(invalid(
            "a one-play Repeat cannot yet carry sound allowances; remove the sound events first",
        ));
    }
    let site = site(document, instance)?;
    let output = crate::repeat_duration(
        FrameDuration::new(site.end - site.start).map_err(DocumentError::from)?,
        plays,
        FrameDuration::ZERO,
    )
    .map_err(DocumentError::from)?;
    let total = document.duration()?.frames();
    total
        .checked_sub(site.end - site.start)
        .and_then(|frames| frames.checked_add(output.frames()))
        .ok_or_else(overflow)?;
    let mut working = document.clone();
    // The first play retains the old root phase and established birth scopes.
    working.audio_bindings = crate::capture_unbound_audio_bindings(document, timing.clone())?;
    if plays > 1 && site.end < total {
        let suffix = next_timing(&working, timing)?;
        let affected = composite::occurrence_shifted_owners(&working, &site.parent, site.slot + 1)?;
        working =
            composite::prepare_owners(&working, &affected, ProjectFrame(site.end), total, &suffix)?;
    }
    let mut result = working.clone();
    let mut repeat = BeatNode::sequence("Repeat", Vec::new());
    repeat.kind = NodeKind::Repeat {
        child: instance.node.clone(),
        iterations: IterationOrder::new(allocation.clone(), plays)?,
        gap: None,
        escalation: None,
    };
    result.nodes.insert(id.clone(), repeat);
    let NodeKind::Sequence { children } = &mut result
        .nodes
        .get_mut(&site.parent)
        .expect("admitted parent")
        .kind
    else {
        unreachable!("admitted Sequence parent")
    };
    children[site.slot] = id.clone();
    // The existing mark transform expresses first-play provenance. The
    // synthetic command is never persisted.
    let command = Command::WrapRepeat {
        node: instance.node.clone(),
        id: id.clone(),
        plays,
        gap: None,
        anchor_policy: WrapAnchorPolicy::First,
    };
    crate::audio_lineage::reconcile(&working, &mut result, &command)?;
    result.marks = crate::marks::transform_marks(&working, &result, &command)?;
    crate::audio_binding_lifecycle::prune(&mut result);
    result.validate()?;
    Ok(result)
}

fn validate_timing(
    document: &ProjectDocument,
    timing: &AudioTimingId,
    allocation: &crate::RevisionId,
) -> Result<(), EditError> {
    if &timing.allocation != allocation {
        return Err(invalid(
            "one-play timing allocation must equal the new revision",
        ));
    }
    if document.audio_bindings().timings.contains_key(timing) {
        return Err(EditError::new(
            EditErrorCode::IdentityConflict,
            "one-play timing identity is already in use",
        ));
    }
    Ok(())
}

fn next_timing(
    document: &ProjectDocument,
    base: &AudioTimingId,
) -> Result<AudioTimingId, EditError> {
    // Reserve the base identity even when capture omitted it.
    let mut next = base.clone();
    loop {
        next.ordinal = next.ordinal.checked_add(1).ok_or_else(|| {
            EditError::new(
                EditErrorCode::LimitExceeded,
                "one-play Repeat requires another timing identity",
            )
        })?;
        if !document.audio_bindings().timings.contains_key(&next) {
            return Ok(next);
        }
    }
}

fn invalid(message: &str) -> EditError {
    EditError::new(EditErrorCode::InvalidCommand, message)
}
fn unavailable(message: &str) -> EditError {
    EditError::new(EditErrorCode::SelectionUnavailable, message)
}
fn overflow() -> EditError {
    EditError::new(EditErrorCode::TimingOverflow, "one-play duration overflow")
}
