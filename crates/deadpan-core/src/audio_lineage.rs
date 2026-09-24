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
    after
        .audio_lineage
        .retain(|id, _| after.nodes.contains_key(id));
    if after.audio_lineage.is_empty()
        || matches!(
            command,
            Command::Split { .. } | Command::Group { .. } | Command::Ungroup { .. }
        )
    {
        return Ok(());
    }
    // Charge the existing structural limits before collecting parent edges;
    // malformed Move/Insert requests must not feed an unbounded graph walk.
    after.structural_durations()?;
    let mut changed = BTreeSet::new();
    for id in before.nodes.keys().chain(after.nodes.keys()) {
        let equal = match (before.nodes.get(id), after.nodes.get(id)) {
            (Some(a), Some(b)) => {
                same_raw_audio(&a.kind, &b.kind)
                    && before.overrides.get(id) == after.overrides.get(id)
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
            NodeKind::Repeat {
                child: ac,
                iterations: ai,
                gap: ag,
            },
            NodeKind::Repeat {
                child: bc,
                iterations: bi,
                gap: bg,
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
