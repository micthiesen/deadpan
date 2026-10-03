//! Exact group ownership, independent of the cursor's picture boundary.

use crate::{
    EditError, EditErrorCode, FrameRange, NodeId, NodeKind, ProjectDocument, SemanticContext,
    SemanticVisualSelection, SliceCaptureSelection,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SemanticTextObject {
    #[serde(deserialize_with = "super::program::deserialize_empty")]
    InnerGroup,
    #[serde(deserialize_with = "super::program::deserialize_empty")]
    AroundGroup,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticObjectSelection {
    pub kind: SemanticTextObject,
    pub group: NodeId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticObjectTarget {
    pub parent: NodeId,
    /// None means an existing InnerGroup has no children. It is an explicit
    /// empty object, never an instruction to fall back to another selector.
    pub selection: Option<SliceCaptureSelection>,
    pub range: FrameRange,
}

impl ProjectDocument {
    pub fn resolve_group_object(
        &self,
        context: &SemanticContext,
        kind: SemanticTextObject,
    ) -> Result<SemanticObjectSelection, EditError> {
        super::planner::validate_context(self, context)?;
        let group = context
            .selected_child
            .as_ref()
            .filter(|node| matches!(self.nodes()[*node].kind, NodeKind::Sequence { .. }))
            .cloned()
            .or_else(|| (context.parent != *self.root()).then(|| context.parent.clone()))
            .ok_or_else(|| unavailable("select a group or enter a group before using ig/ag"))?;
        let object = SemanticObjectSelection { kind, group };
        self.resolve_object_selection(&context.parent, &object)?;
        Ok(object)
    }

    pub fn resolve_object_selection(
        &self,
        navigation_parent: &NodeId,
        object: &SemanticObjectSelection,
    ) -> Result<SemanticObjectTarget, EditError> {
        super::planner::scope_bounds(self, navigation_parent)?;
        let NodeKind::Sequence { children } = &self.nodes()[navigation_parent].kind else {
            unreachable!("scope query admitted an ordinary Sequence")
        };
        if object.group == *self.root()
            || (object.group != *navigation_parent && !children.contains(&object.group))
        {
            return Err(unavailable(
                "the selected group object is outside the current ordinary Sequence",
            ));
        }
        let Some(NodeKind::Sequence { children }) =
            self.nodes().get(&object.group).map(|n| &n.kind)
        else {
            return Err(unavailable(
                "the selected group object is no longer a Sequence",
            ));
        };
        let (start, end) = super::planner::scope_bounds(self, &object.group)?;
        let range = FrameRange::new(start, end).map_err(crate::DocumentError::from)?;
        let (parent, selection) = match object.kind {
            SemanticTextObject::InnerGroup => (
                object.group.clone(),
                children.first().zip(children.last()).map(|(first, last)| {
                    SliceCaptureSelection::Children {
                        first: first.clone(),
                        last: last.clone(),
                    }
                }),
            ),
            SemanticTextObject::AroundGroup => (
                self.parent_of(&object.group)
                    .ok_or_else(|| unavailable("the selected group has no ordinary parent"))?,
                Some(SliceCaptureSelection::Child {
                    node: object.group.clone(),
                }),
            ),
        };
        Ok(SemanticObjectTarget {
            parent,
            selection,
            range,
        })
    }

    pub fn select_semantic_object(
        &self,
        context: &SemanticContext,
        kind: SemanticTextObject,
    ) -> Result<SemanticContext, EditError> {
        let selection = self.resolve_group_object(context, kind)?;
        let target = self.resolve_object_selection(&context.parent, &selection)?;
        let mut result = context.clone();
        result.cursor = target.range.end();
        result.visual_selection = Some(SemanticVisualSelection::Object {
            selection,
            extending: true,
        });
        Ok(result)
    }
}

fn unavailable(message: &str) -> EditError {
    EditError::new(EditErrorCode::SelectionUnavailable, message)
}
