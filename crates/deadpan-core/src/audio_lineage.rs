//! Authored copy continuity, independent of physical node ownership. A lineage
//! origin is an opaque historical name, never a pointer into the current tree.
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    Command, DocumentError, DocumentErrorCode, EditError, HoldRecipe, MAX_DOCUMENT_NODES, NodeId,
    NodeKind, ProjectDocument, RevisionId,
};

/// A logical context retained by transparent copying. This records an authored
/// relationship, not media admission or proof of equal current PCM. A later
/// processing binding must also resolve its clock, live recipes and policies.
/// New relationships use the transaction's never-reused allocation revision.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioLineageId {
    pub allocation: RevisionId,
    pub origin: NodeId,
}

pub(crate) fn validate(document: &ProjectDocument) -> Result<(), DocumentError> {
    if document.audio_lineage.len() > MAX_DOCUMENT_NODES {
        return Err(DocumentError::new(
            DocumentErrorCode::LimitExceeded,
            "audio lineage exceeds the document node limit",
        ));
    }
    if document
        .audio_lineage
        .keys()
        .any(|id| !document.nodes.contains_key(id))
    {
        return Err(DocumentError::new(
            DocumentErrorCode::InvalidTree,
            "audio lineage owner is missing",
        ));
    }
    Ok(())
}

/// Called only for validated transparent context copies, with bounded fresh
/// physical IDs. Seed unchanged originals too so both sides share the relation.
pub(crate) fn inherit(
    document: &mut ProjectDocument,
    mapping: &BTreeMap<NodeId, NodeId>,
    allocation: &RevisionId,
) {
    for (old, new) in mapping {
        let lineage = document
            .audio_lineage
            .entry(old.clone())
            .or_insert_with(|| AudioLineageId {
                allocation: allocation.clone(),
                origin: old.clone(),
            })
            .clone();
        document.audio_lineage.insert(new.clone(), lineage);
    }
}

/// Preserve local identities through relocation, and replace only changed raw
/// contributions and their processing ancestors. Picture, labels, marks and
/// postmapping edge choices do not alter the raw audio context. Group/Ungroup
/// and Split are explicitly transparent, rather than inferred from equal media.
pub(crate) fn reconcile(
    before: &ProjectDocument,
    after: &mut ProjectDocument,
    command: &Command,
) -> Result<(), EditError> {
    reconcile_shared(before, after, command, &mut None)
}

/// [`reconcile`], sharing the structural durations of `after` with the
/// caller's later mark transform (see `marks::transform_marks_shared`).
/// Only `after.audio_lineage` changes here, which no structural check reads.
pub(crate) fn reconcile_shared(
    before: &ProjectDocument,
    after: &mut ProjectDocument,
    command: &Command,
    structure: &mut Option<crate::command::SharedDurations>,
) -> Result<(), EditError> {
    after
        .audio_lineage
        .retain(|id, _| after.nodes.contains_key(id));
    if after.audio_lineage.is_empty()
        || matches!(
            command,
            Command::Split { .. }
                | Command::Group { .. }
                | Command::GroupSelection { .. }
                | Command::Ungroup { .. }
        )
    {
        return Ok(());
    }
    // Charge the existing structural limits before collecting parent edges;
    // malformed Move/Insert requests must not feed an unbounded graph walk.
    if !crate::command_work::local() {
        after.structural_durations()?;
        return reference_closure(before, after);
    }
    crate::command::shared_structure(after, structure)?;
    let changed = changed_closure(before, after);
    after.audio_lineage.retain(|id, _| !changed.contains(id));
    Ok(())
}

fn reference_closure(
    before: &ProjectDocument,
    after: &mut ProjectDocument,
) -> Result<(), EditError> {
    let mut changed = BTreeSet::new();
    for id in before.nodes.keys().chain(after.nodes.keys()) {
        let equal = match (before.nodes.get(id), after.nodes.get(id)) {
            (Some(a), Some(b)) => {
                same_raw_audio(&a.kind, &b.kind)
                    && before.overrides.get(id) == after.overrides.get(id)
                    && before.gap_overrides.get(id) == after.gap_overrides.get(id)
            }
            _ => false,
        };
        if !equal {
            changed.insert(id.clone());
        }
    }
    if changed.is_empty() {
        return Ok(());
    }
    // Each validated tree has at most one parent per node and at most the
    // document edge limit. The union permits both sides of a Move, without
    // repeatedly traversing a shared ancestor for every removed descendant.
    let mut parents: BTreeMap<&NodeId, Vec<&NodeId>> = BTreeMap::new();
    for document in [before, &*after] {
        for parent in document.nodes.keys() {
            for child in document.children(parent) {
                parents.entry(child).or_default().push(parent);
            }
        }
    }
    let mut pending: Vec<_> = changed.iter().cloned().collect();
    while let Some(id) = pending.pop() {
        if let Some(ancestors) = parents.get(&id) {
            for parent in ancestors {
                if changed.insert((*parent).clone()) {
                    pending.push((*parent).clone());
                }
            }
        }
    }
    after.audio_lineage.retain(|id, _| !changed.contains(id));
    Ok(())
}

/// The nodes whose raw audio context changed and all their processing
/// ancestors, exactly as [`reference_closure`] computes them: the same
/// per-node comparison, visited once per identity by merging both sorted
/// node maps rather than looking every identity up twice, and the same union
/// of both documents' parent edges, held in hash maps.
fn changed_closure(before: &ProjectDocument, after: &ProjectDocument) -> BTreeSet<NodeId> {
    let same = |id: &NodeId, a: &crate::BeatNode, b: &crate::BeatNode| {
        same_raw_audio(&a.kind, &b.kind)
            && before.overrides.get(id) == after.overrides.get(id)
            && before.gap_overrides.get(id) == after.gap_overrides.get(id)
    };
    let mut changed: BTreeSet<NodeId> = BTreeSet::new();
    let mut old = before.nodes.iter().peekable();
    let mut new = after.nodes.iter().peekable();
    loop {
        match (old.peek(), new.peek()) {
            (None, None) => break,
            (Some((id, _)), None) => {
                changed.insert((*id).clone());
                old.next();
            }
            (None, Some((id, _))) => {
                changed.insert((*id).clone());
                new.next();
            }
            (Some((a, x)), Some((b, y))) => match a.cmp(b) {
                std::cmp::Ordering::Less => {
                    changed.insert((*a).clone());
                    old.next();
                }
                std::cmp::Ordering::Greater => {
                    changed.insert((*b).clone());
                    new.next();
                }
                std::cmp::Ordering::Equal => {
                    if !same(a, x, y) {
                        changed.insert((*a).clone());
                    }
                    old.next();
                    new.next();
                }
            },
        }
    }
    if changed.is_empty() {
        return changed;
    }
    // Every parent of each child in either document, in the same order as
    // before. Almost every child has one parent per document, so the first
    // two are stored inline and only further ones allocate.
    #[derive(Default)]
    struct Parents<'a> {
        inline: [Option<&'a NodeId>; 2],
        more: Vec<&'a NodeId>,
    }
    let mut parents: crate::id_hash::IdMap<&NodeId, Parents<'_>> =
        crate::id_hash::id_map(before.nodes.len().max(after.nodes.len()));
    for document in [before, after] {
        let plain = document.overrides.is_empty() && document.gap_overrides.is_empty();
        for (parent, node) in &document.nodes {
            let branches = (!plain)
                .then(|| {
                    document
                        .overrides
                        .get(parent)
                        .into_iter()
                        .chain(document.gap_overrides.get(parent))
                        .flat_map(|entries| entries.iter().map(|(_, root)| root))
                })
                .into_iter()
                .flatten();
            for child in node.kind.children().iter().chain(branches) {
                let entry = parents.entry(child).or_default();
                match entry.inline.iter_mut().find(|slot| slot.is_none()) {
                    Some(slot) => *slot = Some(parent),
                    None => entry.more.push(parent),
                }
            }
        }
    }
    let mut pending: Vec<_> = changed.iter().cloned().collect();
    while let Some(id) = pending.pop() {
        if let Some(ancestors) = parents.get(&id) {
            for parent in ancestors.inline.iter().flatten().chain(&ancestors.more) {
                if changed.insert((*parent).clone()) {
                    pending.push((*parent).clone());
                }
            }
        }
    }
    changed
}

fn same_gap(a: Option<&HoldRecipe>, b: Option<&HoldRecipe>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.duration == b.duration && a.audio == b.audio,
        (None, None) => true,
        _ => false,
    }
}

fn same_raw_audio(a: &NodeKind, b: &NodeKind) -> bool {
    match (a, b) {
        (NodeKind::Source { source: a }, NodeKind::Source { source: b }) => {
            a.duration == b.duration
                && a.audio == b.audio
                && a.audio_mapping == b.audio_mapping
                && a.audio_offset == b.audio_offset
        }
        (NodeKind::Hold { recipe: a }, NodeKind::Hold { recipe: b }) => same_gap(Some(a), Some(b)),
        (NodeKind::Sequence { children: a }, NodeKind::Sequence { children: b }) => a == b,
        (
            // Escalation is postmapping gain and framing, not raw audio.
            NodeKind::Repeat {
                child: ac,
                iterations: ai,
                gap: ag,
                ..
            },
            NodeKind::Repeat {
                child: bc,
                iterations: bi,
                gap: bg,
                ..
            },
        ) => ac == bc && ai == bi && same_gap(ag.as_ref(), bg.as_ref()),
        (
            NodeKind::Retime {
                child: ac,
                duration: ad,
                mapping: am,
                pitch: ap,
                purpose: au,
            },
            NodeKind::Retime {
                child: bc,
                duration: bd,
                mapping: bm,
                pitch: bp,
                purpose: bu,
            },
        ) => ac == bc && ad == bd && am == bm && ap == bp && au == bu,
        _ => false,
    }
}
