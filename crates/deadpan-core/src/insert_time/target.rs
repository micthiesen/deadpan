//! Shared structural preflight for native freeze capture and the atomic edit.

use std::collections::BTreeMap;

use crate::{
    EditError, FrameDuration, MAX_DOCUMENT_NODES, NodeId, NodeKind, ProjectDocument, ProjectFrame,
};

/// The owning Sequence for a pause at one committed project boundary. Existing
/// Sequence seams stay at that level; only a strict interior descends further.
/// This is a query result, not an authorization to bypass command validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InsertTimeTarget {
    pub parent: NodeId,
    /// Child slot before the optional Split. A split inserts the Hold at index+1.
    pub index: usize,
    pub split: Option<InsertTimeSplit>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InsertTimeSplit {
    pub target: NodeId,
    pub at: FrameDuration,
    /// Fresh IDs consumed by the internal Split, excluding the new Hold ID.
    pub required_ids: usize,
}

impl ProjectDocument {
    /// Resolve the same parent used by InsertTime without changing the document.
    /// Group ancestors remain live, so freeze capture must retain only picture
    /// scopes below `parent`. Repeat and authored Retime interiors are not yet
    /// admitted; this does not flatten their ownership or round their clocks.
    pub fn insert_time_target(&self, at: ProjectFrame) -> Result<InsertTimeTarget, EditError> {
        resolve(self, &self.durations()?, at)
    }
}

pub(super) fn resolve(
    document: &ProjectDocument,
    durations: &BTreeMap<NodeId, FrameDuration>,
    at: ProjectFrame,
) -> Result<InsertTimeTarget, EditError> {
    let mut parent = document.root();
    let mut position = at;
    loop {
        let NodeKind::Sequence { children } = &document.nodes()[parent].kind else {
            return Err(super::invalid("pause insertion requires a Sequence owner"));
        };
        let boundary = super::root_boundary(children, durations, position)?;
        let split = if let Some((child, cut)) = boundary.interior {
            if matches!(document.nodes()[&child].kind, NodeKind::Sequence { .. }) {
                parent = document
                    .nodes()
                    .get_key_value(&child)
                    .expect("validated child")
                    .0;
                position = ProjectFrame(cut);
                continue;
            }
            super::physical(document, &child)?;
            Some(InsertTimeSplit {
                required_ids: super::split_node_count(document, &child)?,
                target: child,
                at: FrameDuration::new(cut).map_err(crate::DocumentError::from)?,
            })
        } else {
            None
        };
        let required = split.as_ref().map_or(0, |split| split.required_ids);
        if document
            .nodes()
            .len()
            .checked_add(required + 1)
            .is_none_or(|count| count > MAX_DOCUMENT_NODES)
        {
            return Err(super::limit(
                "pause insertion exceeds the document node limit",
            ));
        }
        return Ok(InsertTimeTarget {
            parent: parent.clone(),
            index: boundary.slot,
            split,
        });
    }
}
