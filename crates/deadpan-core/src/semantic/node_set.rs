//! Exact whole-node unions. Structural order, rather than timestamps, keeps
//! empty siblings distinct and makes deletion order deterministic.

use std::collections::BTreeSet;

use crate::{
    EditError, EditErrorCode, FrameDuration, FrameRange, NodeId, NodeKind, ProjectDocument,
    ProjectFrame, SemanticContext, SemanticVisualSelection,
};

pub const MAX_SEMANTIC_NODES: usize = crate::MAX_EDIT_SET_COMPONENTS;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SemanticNodeSetMember {
    pub node: NodeId,
    pub parent: NodeId,
    pub range: FrameRange,
    pub navigation_child: NodeId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticNodeSetTarget {
    pub members: Vec<SemanticNodeSetMember>,
    pub total_duration: FrameDuration,
}

fn unavailable(message: &str) -> EditError {
    EditError::new(EditErrorCode::SelectionUnavailable, message)
}

pub(super) fn validate_nodes(nodes: &[NodeId]) -> Result<(), EditError> {
    if nodes.is_empty() || nodes.len() > MAX_SEMANTIC_NODES {
        return Err(EditError::new(
            EditErrorCode::LimitExceeded,
            "a node set requires 1..=64 authored identities",
        ));
    }
    if nodes.iter().collect::<BTreeSet<_>>().len() != nodes.len() {
        return Err(unavailable("a node set cannot repeat an authored identity"));
    }
    Ok(())
}

pub(super) fn deserialize_nodes<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<NodeId>, D::Error> {
    struct Nodes;
    impl<'de> serde::de::Visitor<'de> for Nodes {
        type Value = Vec<NodeId>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("1..=64 distinct authored node identities")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut input: A,
        ) -> Result<Self::Value, A::Error> {
            let mut nodes = Vec::new();
            while let Some(node) = input.next_element()? {
                if nodes.len() == MAX_SEMANTIC_NODES {
                    return Err(serde::de::Error::custom("node set identity limit"));
                }
                nodes.push(node);
            }
            validate_nodes(&nodes).map_err(serde::de::Error::custom)?;
            Ok(nodes)
        }
    }
    deserializer.deserialize_seq(Nodes)
}

/// Scope-only resolution also validates a finished Visual without recursing
/// through the full context validator. One traversal bounds resolution work.
pub(super) fn resolve_in_scope(
    document: &ProjectDocument,
    context: &SemanticContext,
    nodes: &[NodeId],
) -> Result<SemanticNodeSetTarget, EditError> {
    super::node::edit_authority(context)?;
    validate_nodes(nodes)?;
    let (scope_start, _) = super::planner::scope_bounds(document, &context.parent)?;
    let wanted: BTreeSet<_> = nodes.iter().collect();
    if wanted.contains(document.root()) || wanted.contains(&context.parent) {
        return Err(unavailable(
            "node set members must be strict descendants of the current Sequence",
        ));
    }
    let durations = document.durations_shared()?;
    let mut members = Vec::with_capacity(nodes.len());
    // (node, immediate parent, start, navigation child, selected ancestor).
    let mut pending = vec![(
        context.parent.clone(),
        context.parent.clone(),
        scope_start,
        context.parent.clone(),
        false,
    )];
    let mut total = 0i64;
    let mut visited = 0usize;
    while let Some((node, parent, start, navigation_child, ancestor)) = pending.pop() {
        visited += 1;
        if visited > crate::MAX_DOCUMENT_NODES {
            return Err(unavailable(
                "node set traversal exceeds the document node limit",
            ));
        }
        let selected = wanted.contains(&node);
        let end = ProjectFrame(
            start
                .0
                .checked_add(durations[&node].frames())
                .ok_or_else(|| unavailable("node set boundary overflows"))?,
        );
        if selected {
            if ancestor {
                return Err(unavailable("node set members cannot contain one another"));
            }
            total = total
                .checked_add(durations[&node].frames())
                .ok_or_else(|| unavailable("node set duration overflows"))?;
            members.push(SemanticNodeSetMember {
                node: node.clone(),
                parent,
                range: FrameRange::new(start, end).map_err(crate::DocumentError::from)?,
                navigation_child: navigation_child.clone(),
            });
        }
        if let NodeKind::Sequence { children } = &document.nodes()[&node].kind {
            let mut child_end = end;
            for child in children.iter().rev() {
                let child_start = ProjectFrame(
                    child_end
                        .0
                        .checked_sub(durations[child].frames())
                        .ok_or_else(|| unavailable("node set boundary overflows"))?,
                );
                pending.push((
                    child.clone(),
                    node.clone(),
                    child_start,
                    if node == context.parent {
                        child.clone()
                    } else {
                        navigation_child.clone()
                    },
                    ancestor || selected,
                ));
                child_end = child_start;
            }
        }
    }
    if members.len() != nodes.len() {
        return Err(unavailable(
            "a node set member is missing, outside this Sequence, or inside a Repeat, Retime or partition owner",
        ));
    }
    Ok(SemanticNodeSetTarget {
        members,
        total_duration: FrameDuration::new(total).map_err(crate::DocumentError::from)?,
    })
}

impl ProjectDocument {
    pub fn resolve_semantic_nodes(
        &self,
        context: &SemanticContext,
        nodes: &[NodeId],
    ) -> Result<SemanticNodeSetTarget, EditError> {
        super::node::edit_authority(context)?;
        super::planner::validate_context(self, context)?;
        resolve_in_scope(self, context, nodes)
    }

    pub fn select_semantic_nodes(
        &self,
        context: &SemanticContext,
        nodes: Vec<NodeId>,
    ) -> Result<SemanticContext, EditError> {
        let mut result = context.clone();
        result.visual_selection = None;
        let target = self.resolve_semantic_nodes(&result, &nodes)?;
        let last = target.members.last().expect("nonempty checked set");
        result.cursor = last.range.end();
        result.selected_child = Some(last.navigation_child.clone());
        result.visual_selection = Some(SemanticVisualSelection::Nodes { nodes });
        Ok(result)
    }
}
