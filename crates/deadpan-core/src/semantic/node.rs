//! Whole authored node identity, independent of cursor and occurrence guesses.

use crate::{
    EditError, EditErrorCode, FrameRange, NodeId, NodeKind, ProjectDocument, SemanticContext,
    SemanticVisualSelection,
};

/// A whole node beneath ordinary Sequence ancestry in the captured scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticNodeTarget {
    /// The actual immediate Sequence parent used for capture and mutation.
    pub parent: NodeId,
    pub range: FrameRange,
    /// The direct child containing the target in the captured navigation scope.
    pub navigation_child: NodeId,
}

fn unavailable(message: &str) -> EditError {
    EditError::new(EditErrorCode::SelectionUnavailable, message)
}

pub(super) fn edit_authority(context: &SemanticContext) -> Result<(), EditError> {
    if context.original.is_some()
        || context.scoped.is_some()
        || context.hold_duration.is_some()
        || context.shared_hold_duration.is_some()
        || context.mixed_hold_duration.is_some()
        || context.selected_hold_duration.is_some()
    {
        return Err(unavailable(
            "node selection requires ordinary Your edit selection authority",
        ));
    }
    Ok(())
}

/// Scope-only validation avoids recursing through a finished Node Visual.
pub(super) fn resolve_in_scope(
    document: &ProjectDocument,
    context: &SemanticContext,
    node: &NodeId,
) -> Result<SemanticNodeTarget, EditError> {
    edit_authority(context)?;
    super::planner::scope_bounds(document, &context.parent)?;
    if node == document.root() || node == &context.parent {
        return Err(unavailable(
            "the selected node must be a strict descendant of the current Sequence",
        ));
    }
    // Historical audio owners reserve identities, but are not authored nodes.
    if !document.nodes().contains_key(node) {
        return Err(unavailable(
            "the selected authored node is no longer present",
        ));
    }
    let mut child = node.clone();
    let mut immediate_parent = None;
    for _ in 0..crate::MAX_DOCUMENT_DEPTH {
        let parent = document
            .parent_of(&child)
            .ok_or_else(|| unavailable("the selected node is outside the current Sequence"))?;
        if !matches!(document.nodes()[&parent].kind, NodeKind::Sequence { .. }) {
            return Err(unavailable(
                "node selection cannot cross a Repeat, Retime or partition owner",
            ));
        }
        immediate_parent.get_or_insert_with(|| parent.clone());
        if parent == context.parent {
            let parent = immediate_parent.expect("the target has an immediate parent");
            let range = document.sequence_children(&parent, node, node)?.range;
            return Ok(SemanticNodeTarget {
                parent,
                range,
                navigation_child: child,
            });
        }
        child = parent;
    }
    Err(unavailable(
        "the selected node is outside the current Sequence or exceeds its ancestry limit",
    ))
}

impl ProjectDocument {
    /// Resolve one live authored identity against this staged document and the
    /// checked invocation scope. Empty Sequences retain their structural slot.
    pub fn resolve_semantic_node(
        &self,
        context: &SemanticContext,
        node: &NodeId,
    ) -> Result<SemanticNodeTarget, EditError> {
        edit_authority(context)?;
        super::planner::validate_context(self, context)?;
        resolve_in_scope(self, context, node)
    }

    /// Select a finished whole node while retaining ordinary navigation scope.
    /// The saved intent contains only its identity, never resolved timestamps.
    pub fn select_semantic_node(
        &self,
        context: &SemanticContext,
        node: NodeId,
    ) -> Result<SemanticContext, EditError> {
        let mut result = context.clone();
        // Explicit replacement must not depend on an old Visual still resolving.
        result.visual_selection = None;
        let target = self.resolve_semantic_node(&result, &node)?;
        result.cursor = target.range.end();
        result.selected_child = Some(target.navigation_child);
        result.visual_selection = Some(SemanticVisualSelection::Node { node });
        Ok(result)
    }
}
