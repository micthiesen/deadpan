//! Authored creative seams, independent of raw Source and allocation support.

use std::collections::BTreeMap;

use crate::{
    AudioEditorialEdges, EditError, EditErrorCode, FrameDuration, NodeId, NodeKind, ProjectDocument,
};

/// Mark the changed allocation sides and their actual incident neighbors.
/// The complete structural edit is already installed; marks and bindings may
/// still await their final transforms, so inspect only validated structure.
pub(crate) fn mark_edges(
    document: &mut ProjectDocument,
    parent: &NodeId,
    node: &NodeId,
    edges: AudioEditorialEdges,
) -> Result<(), EditError> {
    let durations = document.structural_durations()?;
    let mut parents = BTreeMap::new();
    for (id, beat) in document.nodes() {
        for child in beat.kind.children() {
            parents.insert(child, id);
        }
    }
    let mut updates = vec![(node.clone(), edges)];
    for (active, start) in [(edges.start, true), (edges.end, false)] {
        if active
            && let Some(neighbor) = neighbor(document, &parents, &durations, parent, node, start)?
        {
            updates.push((
                neighbor,
                AudioEditorialEdges {
                    start: !start,
                    end: start,
                },
            ));
        }
    }
    for (owner, edges) in updates {
        let current = &mut document
            .nodes
            .get_mut(&owner)
            .ok_or_else(|| invalid("source edit lost an editorial seam owner"))?
            .audio_editorial_edges;
        current.start |= edges.start;
        current.end |= edges.end;
    }
    Ok(())
}

/// Empty siblings have no output side. A positive silent child remains the
/// actual neighbor and stops the search instead of fading a distant voice.
fn neighbor(
    document: &ProjectDocument,
    parents: &BTreeMap<&NodeId, &NodeId>,
    durations: &BTreeMap<NodeId, FrameDuration>,
    parent: &NodeId,
    node: &NodeId,
    start: bool,
) -> Result<Option<NodeId>, EditError> {
    let mut current = node;
    let mut parent = parent;
    loop {
        let Some(NodeKind::Sequence { children }) =
            document.nodes().get(parent).map(|node| &node.kind)
        else {
            return Err(invalid(
                "source edit neighbor scope must be an ordinary Sequence",
            ));
        };
        let slot = children
            .iter()
            .position(|child| child == current)
            .ok_or_else(|| invalid("source edit lost its incident child"))?;
        let found = if start {
            children[..slot]
                .iter()
                .rev()
                .find(|child| durations[*child] != FrameDuration::ZERO)
        } else {
            children[slot + 1..]
                .iter()
                .find(|child| durations[*child] != FrameDuration::ZERO)
        };
        if let Some(found) = found {
            return Ok(Some(found.clone()));
        }
        let Some(ancestor) = parents.get(parent) else {
            return Ok(None);
        };
        current = parent;
        parent = ancestor;
    }
}

fn invalid(message: &str) -> EditError {
    EditError::new(EditErrorCode::InvalidCommand, message)
}
