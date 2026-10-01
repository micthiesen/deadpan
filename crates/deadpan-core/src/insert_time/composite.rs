//! Sequence insertion moves each physical owner's allocated entries on its
//! current clock. Live ancestor groups remain intact. Repeats stay compact and
//! Preserve preparation in shifted siblings stays local.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    AudioPlacementTemplate, AudioReanchorStep, AudioTimingId, BeatNode, EditError, ExactFrameRange,
    ExactRatio, MAX_AUDIO_BINDING_ENTRIES, MAX_AUDIO_BINDING_TERMS, MAX_DOCUMENT_NODES, NodeId,
    NodeKind, OwnedAudioBinding, PitchPolicy, ProjectDocument, ProjectFrame, RevisionId,
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
    let captured =
        crate::audio_binding_lifecycle::capture_for_composite_insertion(document, timing.clone())?;
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
            &mut working.audio_bindings.bindings,
            captured.node_placements,
            &affected,
            window,
            &mut entries,
        )?;
        append_steps(
            &mut working.audio_bindings.gap_bindings,
            captured.gap_placements,
            &affected,
            window,
            &mut entries,
        )?;
    }
    crate::audio_binding_lifecycle::prune(&mut working);
    working.audio_bindings.validate_for(&working)?;
    Ok(working)
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
            NodeKind::Retime { duration, mapping, pitch: PitchPolicy::Preserve, .. }
                if mapping.duration() != *duration
        );
        if !preserve {
            pending.extend(document.children(id));
        }
    }
    Ok(affected)
}

pub(crate) fn append_steps(
    bindings: &mut BTreeMap<NodeId, OwnedAudioBinding>,
    placements: BTreeMap<NodeId, AudioPlacementTemplate>,
    affected: &BTreeSet<NodeId>,
    window: ExactFrameRange,
    entries: &mut usize,
) -> Result<(), EditError> {
    for (owner, placement) in placements {
        if !affected.contains(&owner) {
            continue;
        }
        let binding = bindings.get_mut(&owner).ok_or_else(|| {
            super::invalid("composite pause is missing a captured physical binding")
        })?;
        let terms = binding
            .resume
            .as_ref()
            .map_or(0, |resume| resume.phase.terms.len());
        if terms + binding.reanchors.len() >= MAX_AUDIO_BINDING_TERMS {
            return Err(super::limit("composite pause exceeds the reanchor limit"));
        }
        *entries = entries
            .checked_add(placement.entry_count())
            .filter(|count| *count <= MAX_AUDIO_BINDING_ENTRIES)
            .ok_or_else(|| super::limit("composite pause binding entries"))?;
        binding.reanchors.push(AudioReanchorStep {
            anchor: Default::default(),
            placement,
            window: Some(window),
        });
    }
    Ok(())
}
