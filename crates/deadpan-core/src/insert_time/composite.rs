//! Sequence insertion moves each physical owner's allocated entries on its
//! current clock. Live ancestor groups remain intact. Repeats stay compact and
//! Preserve preparation in shifted siblings stays local.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    AudioBindingState, AudioPlacementTemplate, AudioReanchorStep, AudioRecipeKind, AudioTimingId,
    BeatNode, EditError, ExactFrameRange, ExactRatio, MAX_AUDIO_BINDING_ENTRIES,
    MAX_AUDIO_BINDING_TERMS, MAX_DOCUMENT_NODES, NodeId, NodeKind, ProjectDocument, ProjectFrame,
    RevisionId,
};

pub(super) struct Insertion<'a> {
    pub at: ProjectFrame,
    pub total: i64,
    pub node: BeatNode,
    pub id: &'a NodeId,
    pub timing: &'a AudioTimingId,
    pub allocation: &'a RevisionId,
}

pub(super) fn apply_at(
    document: &ProjectDocument,
    parent: &NodeId,
    slot: usize,
    insertion: Insertion<'_>,
) -> Result<ProjectDocument, EditError> {
    if document.nodes().len() >= MAX_DOCUMENT_NODES {
        return Err(super::limit(
            "pause insertion exceeds the document node limit",
        ));
    }
    let working = prepare_suffix(
        document,
        parent,
        slot,
        insertion.at,
        insertion.total,
        insertion.timing,
    )?;
    super::insert_leaf_at(
        &working,
        parent,
        slot,
        insertion.id,
        insertion.node,
        insertion.allocation,
    )
}

/// Capture each retained suffix owner's old entry before changing the tree.
/// Replacement uses this on the split, undeleted tree so there is no shorter
/// intermediate clock that could discard the suffix's last rounded sample.
pub(crate) fn prepare_suffix(
    document: &ProjectDocument,
    parent: &NodeId,
    slot: usize,
    at: ProjectFrame,
    total: i64,
    timing: &AudioTimingId,
) -> Result<ProjectDocument, EditError> {
    let affected = shifted_owners(document, parent, slot)?;
    prepare_owners(document, &affected, at, total, timing)
}

/// Capture the old entries of `affected` owners in the pre-edit root window
/// `[at, total)` before changing the tree, so each keeps its own clock.
pub(crate) fn prepare_owners(
    document: &ProjectDocument,
    affected: &BTreeSet<NodeId>,
    at: ProjectFrame,
    total: i64,
    timing: &AudioTimingId,
) -> Result<ProjectDocument, EditError> {
    // Only the new bindings and the affected owners' steps read the new
    // table, so it may be a provisional slice (see `capture_scoped`).
    let captured = crate::audio_binding_lifecycle::capture_for_composite_insertion_scoped(
        document,
        timing.clone(),
        affected,
    )?;
    let mut working = document.clone();
    working.audio_bindings = captured.state;
    if let Some(layout) = captured.phase_only_layout {
        working
            .audio_bindings
            .timings
            .insert(timing.clone(), layout);
    }
    // A terminal zero-duration Sequence has no physical output to reanchor.
    if at.0 < total {
        let window = ExactFrameRange::new(ExactRatio::integer(at.0), ExactRatio::integer(total))?;
        let mut entries = 0usize;
        for (_, _, binding) in working.audio_bindings.owners() {
            for placement in binding.placements() {
                entries = entries
                    .checked_add(placement.entry_count())
                    .filter(|count| *count <= MAX_AUDIO_BINDING_ENTRIES)
                    .ok_or_else(|| super::limit("composite pause binding entries"))?;
            }
        }
        append_steps(
            &mut working.audio_bindings,
            AudioRecipeKind::Node,
            captured.node_placements,
            affected,
            window,
            &mut entries,
        )?;
        append_steps(
            &mut working.audio_bindings,
            AudioRecipeKind::RepeatGap,
            captured.gap_placements,
            affected,
            window,
            &mut entries,
        )?;
    }
    crate::audio_binding_lifecycle::prune(&mut working);
    // The steps above name only captured aliases; extend the table anyway if
    // any template reads one it does not project.
    crate::audio_binding_lifecycle::rescope_timing(document, &mut working.audio_bindings, timing)?;
    working.audio_bindings.validate_for(&working)?;
    Ok(working)
}

/// Every physical owner inside `repeat`, including the Repeat itself for its
/// default gap. Descent stops below a Preserve stage, as for a suffix.
pub(crate) fn repeat_interior_owners(
    document: &ProjectDocument,
    repeat: &NodeId,
) -> Result<BTreeSet<NodeId>, EditError> {
    let mut pending = vec![repeat];
    let mut affected = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if affected.len() >= MAX_DOCUMENT_NODES || !affected.insert(id.clone()) {
            return Err(super::limit("Repeat interior traversal"));
        }
        let node = &document.nodes()[id];
        let preserve = matches!(
            &node.kind,
            NodeKind::Retime { duration, mapping, pitch, .. }
                if pitch.processes(mapping.duration() == *duration)
        );
        if !preserve {
            pending.extend(document.children(id));
        }
    }
    Ok(affected)
}

pub(crate) fn shifted_owners(
    document: &ProjectDocument,
    parent: &NodeId,
    slot: usize,
) -> Result<BTreeSet<NodeId>, EditError> {
    let NodeKind::Sequence { children } = &document.nodes()[parent].kind else {
        return Err(super::invalid("pause insertion parent is not a Sequence"));
    };
    let mut pending: Vec<_> = children[slot..].iter().collect();
    if parent != document.root() {
        let index = crate::AnchorIndex::from_durations(document, document.durations()?)?;
        let mut child = parent;
        while child != document.root() {
            let (ancestor, _) = index
                .parents
                .get(child)
                .ok_or_else(|| super::invalid("pause parent is outside the project"))?;
            let NodeKind::Sequence { children } = &document.nodes()[ancestor].kind else {
                return Err(super::invalid(
                    "pause insertion cannot change a Repeat or Retime ancestor",
                ));
            };
            let position = children
                .iter()
                .position(|candidate| candidate == child)
                .ok_or_else(|| super::invalid("pause parent is missing from its ancestor"))?;
            pending.extend(children[position + 1..].iter());
            child = ancestor;
        }
    }
    let mut affected = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if affected.len() >= MAX_DOCUMENT_NODES || !affected.insert(id.clone()) {
            return Err(super::limit("composite pause traversal"));
        }
        let node = &document.nodes()[id];
        let preserve = matches!(
            &node.kind,
            NodeKind::Retime { duration, mapping, pitch, .. }
                if pitch.processes(mapping.duration() == *duration)
        );
        if !preserve {
            pending.extend(document.children(id));
        }
    }
    Ok(affected)
}

pub(crate) fn append_steps(
    state: &mut AudioBindingState,
    kind: AudioRecipeKind,
    placements: BTreeMap<NodeId, AudioPlacementTemplate>,
    affected: &BTreeSet<NodeId>,
    window: ExactFrameRange,
    entries: &mut usize,
) -> Result<(), EditError> {
    append_anchored_steps(
        state,
        kind,
        placements,
        affected,
        Default::default(),
        Some(window),
        entries,
    )
}

/// Shared bounded chronological append. Source endpoints keep window=None;
/// validation of the complete binding enforces their captured Source owner.
///
/// A step that provably cannot change its owner's resolution in any
/// occurrence is not stored (see `reanchor_step_is_inert`). Moving every
/// suffix owner would otherwise add one step per owner per edit, and keep
/// each edit's pre-edit timing table alive, although an unsplit owner's entry
/// is already its resume anchor. Steps under Repeats are always kept.
pub(crate) fn append_anchored_steps(
    state: &mut AudioBindingState,
    kind: AudioRecipeKind,
    placements: BTreeMap<NodeId, AudioPlacementTemplate>,
    affected: &BTreeSet<NodeId>,
    anchor: crate::AudioReanchorAnchor,
    window: Option<ExactFrameRange>,
    entries: &mut usize,
) -> Result<(), EditError> {
    if !anchor.is_allocation_entry() && window.is_some() {
        return Err(super::invalid(
            "Source endpoint cannot carry an allocation window",
        ));
    }
    for (owner, placement) in placements {
        if !affected.contains(&owner) {
            continue;
        }
        let bindings = match kind {
            AudioRecipeKind::Node => &state.bindings,
            AudioRecipeKind::RepeatGap => &state.gap_bindings,
        };
        let binding = bindings.get(&owner).ok_or_else(|| {
            super::invalid("composite pause is missing a captured physical binding")
        })?;
        let terms = binding
            .resume
            .as_ref()
            .map_or(0, |resume| resume.phase.terms.len());
        if terms + binding.reanchors.len() >= MAX_AUDIO_BINDING_TERMS {
            return Err(super::limit("composite pause exceeds the reanchor limit"));
        }
        let step = AudioReanchorStep {
            anchor,
            placement,
            window,
        };
        if kind == AudioRecipeKind::Node
            && crate::audio_binding_lifecycle::compact_representation()
            && state.reanchor_step_is_inert(&owner, binding, &step)
        {
            continue;
        }
        *entries = entries
            .checked_add(step.placement.entry_count())
            .filter(|count| *count <= MAX_AUDIO_BINDING_ENTRIES)
            .ok_or_else(|| super::limit("composite pause binding entries"))?;
        let bindings = match kind {
            AudioRecipeKind::Node => &mut state.bindings,
            AudioRecipeKind::RepeatGap => &mut state.gap_bindings,
        };
        bindings
            .get_mut(&owner)
            .expect("binding was found above")
            .reanchors
            .push(step);
    }
    Ok(())
}
