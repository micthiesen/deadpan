//! Sequence insertion places the new leaf at its slot. Original audio follows
//! the current tree, so later owners need no retained clock; the suffix walks
//! below still refuse edits that would change a Repeat or Retime ancestor.

use std::collections::BTreeSet;

use crate::{
    BeatNode, EditError, MAX_DOCUMENT_NODES, NodeId, NodeKind, ProjectDocument, RevisionId,
};

pub(super) struct Insertion<'a> {
    pub node: BeatNode,
    pub id: &'a NodeId,
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
    check_suffix(document, parent, slot)?;
    super::insert_leaf_at(
        document,
        parent,
        slot,
        insertion.id,
        insertion.node,
        insertion.allocation,
    )
}

/// Refuse an insertion whose suffix would cross a Repeat or Retime ancestor.
pub(crate) fn check_suffix(
    document: &ProjectDocument,
    parent: &NodeId,
    slot: usize,
) -> Result<(), EditError> {
    shifted_owners(document, parent, slot).map(|_| ())
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
    shifted_owners_with_repeats(document, parent, slot, false)
}

pub(crate) fn occurrence_shifted_owners(
    document: &ProjectDocument,
    parent: &NodeId,
    slot: usize,
) -> Result<BTreeSet<NodeId>, EditError> {
    shifted_owners_with_repeats(document, parent, slot, true)
}

/// The suffix after one concrete owner, including later ancestor plays/gaps.
/// The supplied root window excludes the owner's own interior. In particular,
/// a Repeat may directly own another Repeat without a Sequence seam between.
pub(crate) fn occurrence_owners_after(
    document: &ProjectDocument,
    owner: &NodeId,
) -> Result<BTreeSet<NodeId>, EditError> {
    let mut child = owner;
    let mut pending = Vec::new();
    for _ in 0..=crate::MAX_DOCUMENT_DEPTH {
        if child == document.root() {
            let mut affected = BTreeSet::new();
            for root in pending {
                affected.extend(repeat_interior_owners(document, root)?);
                if affected.len() > MAX_DOCUMENT_NODES {
                    return Err(super::limit("occurrence suffix traversal"));
                }
            }
            return Ok(affected);
        }
        let ancestor = document
            .parent_of(child)
            .ok_or_else(|| super::invalid("pause owner is outside the project"))?;
        let (ancestor, node) = document
            .nodes()
            .get_key_value(&ancestor)
            .expect("validated parent");
        match &node.kind {
            NodeKind::Repeat { .. } => {
                pending.clear();
                pending.push(ancestor);
            }
            NodeKind::Sequence { children } => {
                let position = children
                    .iter()
                    .position(|candidate| candidate == child)
                    .ok_or_else(|| super::invalid("pause owner lost its ancestor slot"))?;
                pending.extend(children[position + 1..].iter());
            }
            _ => {
                return Err(super::invalid(
                    "pause insertion cannot change a Retime ancestor",
                ));
            }
        }
        child = ancestor;
    }
    Err(super::limit("occurrence suffix depth"))
}

fn shifted_owners_with_repeats(
    document: &ProjectDocument,
    parent: &NodeId,
    slot: usize,
    repeat_ancestors: bool,
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
            if repeat_ancestors
                && matches!(document.nodes()[ancestor].kind, NodeKind::Repeat { .. })
            {
                // The selected owned branch grows once. All later concrete
                // plays and gaps keep their pre-edit entries; the capture's
                // root window excludes earlier occurrences of shared owners.
                pending.clear();
                pending.push(
                    document
                        .nodes()
                        .get_key_value(ancestor)
                        .expect("validated ancestor")
                        .0,
                );
                child = ancestor;
                continue;
            }
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
