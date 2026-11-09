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
        let message = if matches!(
            document.nodes()[&instance.node].kind,
            NodeKind::Repeat { .. }
        ) {
            "this Repeat is the play's whole contents, so there is nothing around it to move; change it from the parent (Backspace) for every play"
        } else {
            "this beat is the play's whole contents; split it first, or press Enter to edit a beat in its group"
        };
        return Err(unavailable(message));
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

/// Change an existing Repeat's total plays in its play only. Surviving plays
/// keep their stable identities and positions; new plays take this revision's
/// allocation. Everything after the Repeat keeps its own clock, as for the
/// root setter.
pub(crate) fn set_repeat_plays(
    original: &ProjectDocument,
    document: &ProjectDocument,
    instance: &InstancePath,
    plays: u32,
    timing: &AudioTimingId,
    allocation: &crate::RevisionId,
) -> Result<ProjectDocument, EditError> {
    let current = repeat_plays(document, &instance.node, plays)?;
    if current == plays {
        return Err(unchanged(plays));
    }
    validate_timing(document, timing, allocation)?;
    // Growth names plays with this revision's allocation; isolation copies
    // only reuse existing play orders.
    if original.nodes().values().any(|node| {
        matches!(&node.kind, NodeKind::Repeat { iterations, .. }
            if iterations.segments().any(|(existing, _, _)| existing == allocation))
    }) {
        return Err(EditError::new(
            EditErrorCode::IdentityConflict,
            "Repeat reuses an existing allocation",
        ));
    }
    if !document.sound_allowances.is_empty() {
        return Err(invalid(
            "a one-play Repeat count cannot yet carry sound allowances; remove the sound events first",
        ));
    }
    let site = site(document, instance)?;
    let NodeKind::Repeat { gap, .. } = &document.nodes()[&instance.node].kind else {
        unreachable!("admitted Repeat")
    };
    let gap = gap.clone();
    let command = Command::SetRepeat {
        node: instance.node.clone(),
        plays,
        gap,
    };
    let total = document.duration()?.frames();
    let mut working = document.clone();
    working.audio_bindings = crate::capture_unbound_audio_bindings(document, timing.clone())?;
    // A Repeat child has positive duration, so any new count moves the end.
    if site.end < total {
        let suffix = next_timing(&working, timing)?;
        let affected = composite::occurrence_shifted_owners(&working, &site.parent, site.slot + 1)?;
        working =
            composite::prepare_owners(&working, &affected, ProjectFrame(site.end), total, &suffix)?;
    }
    let mut result = working.clone();
    crate::command::reduce(&mut result, &command, allocation)?;
    crate::audio_lineage::reconcile(&working, &mut result, &command)?;
    result.marks = crate::marks::transform_marks(&working, &result, &command)?;
    crate::audio_binding_lifecycle::prune(&mut result);
    result.validate()?;
    Ok(result)
}

/// Change an existing Repeat's total plays and complete gap set in its play
/// only, as the root `SetRepeatPlays` then `SetRepeatGaps` would: surviving
/// plays keep their identities, named gaps become fresh independent Holds and
/// unnamed branches end. Later plays and gaps inside the Repeat and everything
/// after it keep their own pre-edit clocks.
#[allow(clippy::too_many_arguments)]
pub(crate) fn set_repeat_gaps(
    original: &ProjectDocument,
    document: &ProjectDocument,
    instance: &InstancePath,
    plays: u32,
    gap: Option<&crate::HoldRecipe>,
    branches: &[crate::RepeatGapHold],
    timing: &AudioTimingId,
    allocation: &crate::RevisionId,
) -> Result<ProjectDocument, EditError> {
    let node = &instance.node;
    let current = repeat_plays(document, node, plays)?;
    if gaps_unchanged(document, node, plays, gap, branches) {
        return Err(unchanged_gaps());
    }
    validate_timing(document, timing, allocation)?;
    if original.nodes().values().any(|node| {
        matches!(&node.kind, NodeKind::Repeat { iterations, .. }
            if iterations.segments().any(|(existing, _, _)| existing == allocation))
    }) {
        return Err(EditError::new(
            EditErrorCode::IdentityConflict,
            "Repeat reuses an existing allocation",
        ));
    }
    if !document.sound_allowances.is_empty() {
        return Err(invalid(
            "one-play Repeat gaps cannot yet carry sound allowances; remove the sound events first",
        ));
    }
    let site = site(document, instance)?;
    // The structural change alone, on a trial copy: the count first, so the
    // named gaps follow the plays that will exist.
    let reduce = |target: &mut ProjectDocument| -> Result<(), EditError> {
        if plays != current {
            let NodeKind::Repeat { gap: old, .. } = &target.nodes()[node].kind else {
                unreachable!("admitted Repeat")
            };
            let command = Command::SetRepeat {
                node: node.clone(),
                plays,
                gap: old.clone(),
            };
            crate::command::reduce(target, &command, allocation)?;
        }
        crate::repeat_selection::validate_gap_request(target, node, gap, branches)?;
        crate::repeat_selection::reduce_gaps(target, node, gap, branches, allocation)
    };
    let mut trial = document.clone();
    reduce(&mut trial)?;
    crate::audio_binding_lifecycle::prune(&mut trial);
    let duration = trial.durations()?[node].frames();
    let total = document.duration()?.frames();
    let mut working = document.clone();
    working.audio_bindings = crate::capture_unbound_audio_bindings(document, timing.clone())?;
    // Later plays and gaps move with the gaps before them; each keeps its
    // own pre-edit clock, captured once before the tree changes.
    let first_end =
        crate::repeat_selection::first_play_end(document, node, ProjectFrame(site.start))?;
    if first_end.0 < site.end {
        let interior = next_timing(&working, timing)?;
        working = composite::prepare_owners(
            &working,
            &composite::repeat_interior_owners(&working, node)?,
            first_end,
            site.end,
            &interior,
        )?;
    }
    if duration != site.end - site.start && site.end < total {
        let suffix = next_timing(&working, timing)?;
        let affected = composite::occurrence_shifted_owners(&working, &site.parent, site.slot + 1)?;
        working =
            composite::prepare_owners(&working, &affected, ProjectFrame(site.end), total, &suffix)?;
    }
    let mut result = working.clone();
    reduce(&mut result)?;
    crate::audio_binding_lifecycle::prune(&mut result);
    let command = Command::SetRepeat {
        node: node.clone(),
        plays,
        gap: gap.cloned(),
    };
    crate::audio_lineage::reconcile(&working, &mut result, &command)?;
    result.marks = crate::marks::transform_marks(&working, &result, &command)?;
    crate::audio_binding_lifecycle::prune(&mut result);
    result.validate()?;
    Ok(result)
}

/// Whether the request leaves the Repeat's count and every gap as they are.
pub(crate) fn gaps_unchanged(
    document: &ProjectDocument,
    node: &NodeId,
    plays: u32,
    gap: Option<&crate::HoldRecipe>,
    branches: &[crate::RepeatGapHold],
) -> bool {
    let requested: Vec<_> = branches
        .iter()
        .map(|branch| (branch.after.clone(), branch.hold.clone()))
        .collect();
    matches!(&document.nodes()[node].kind,
        NodeKind::Repeat { iterations, .. } if iterations.len() == plays)
        && document.repeat_gaps_unchanged(node, gap, &requested)
}

pub(crate) fn unchanged_gaps() -> EditError {
    unavailable("the Repeat already has these plays and gaps in this play; no edit was made")
}

/// The selected Repeat's current total plays, refusing other beats.
pub(crate) fn repeat_plays(
    document: &ProjectDocument,
    node: &NodeId,
    plays: u32,
) -> Result<u32, EditError> {
    if plays == 0 {
        return Err(invalid("Repeat total plays must be positive"));
    }
    match &document.nodes()[node].kind {
        NodeKind::Repeat { iterations, .. } => Ok(iterations.len()),
        _ => Err(EditError::new(
            EditErrorCode::WrongNodeKind,
            "setting total plays in one play needs a selected Repeat",
        )),
    }
}

pub(crate) fn unchanged(plays: u32) -> EditError {
    unavailable(&format!(
        "the Repeat already plays {plays} times in this play; no edit was made"
    ))
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
