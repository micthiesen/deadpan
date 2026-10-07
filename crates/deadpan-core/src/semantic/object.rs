//! Exact group and beat ownership, independent of the cursor's picture
//! boundary. `ig`/`ag` select a group's contents or the whole group; `ib`/`ab`
//! select one beat with identical picture time, differing only in whether its
//! owned temporal attachments come along.

use crate::{
    EditError, EditErrorCode, FrameRange, NodeId, NodeKind, ProjectDocument, ProjectFrame,
    SemanticContext, SemanticVisualSelection, SliceAttachments, SliceCaptureSelection,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SemanticTextObject {
    #[serde(deserialize_with = "super::program::deserialize_empty")]
    InnerGroup,
    #[serde(deserialize_with = "super::program::deserialize_empty")]
    AroundGroup,
    /// `ib`: the beat's pictures, linked audio, structure and effects, without
    /// its owned temporal attachments.
    #[serde(deserialize_with = "super::program::deserialize_empty")]
    InnerBeat,
    /// `ab`: the same beat with its captions, cutaways, beat sounds and marks.
    #[serde(deserialize_with = "super::program::deserialize_empty")]
    AroundBeat,
}

impl SemanticTextObject {
    /// Beat objects name one direct child; group objects name a Sequence.
    pub const fn is_beat(self) -> bool {
        matches!(self, Self::InnerBeat | Self::AroundBeat)
    }

    /// A short lowercase phrase naming what the object selects.
    pub const fn noun(self) -> &'static str {
        match self {
            Self::InnerGroup => "group contents",
            Self::AroundGroup => "whole group",
            Self::InnerBeat => "beat without attachments",
            Self::AroundBeat => "beat with attachments",
        }
    }

    /// Whether a capture of this object keeps owned temporal attachments.
    pub const fn attachments(self) -> SliceAttachments {
        match self {
            Self::InnerBeat => SliceAttachments::Excluded,
            Self::InnerGroup | Self::AroundGroup | Self::AroundBeat => SliceAttachments::Owned,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticObjectSelection {
    pub kind: SemanticTextObject,
    /// The selected group for `ig`/`ag`, or the selected beat for `ib`/`ab`.
    pub group: NodeId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticObjectTarget {
    pub parent: NodeId,
    /// None means an existing InnerGroup has no children. It is an explicit
    /// empty object, never an instruction to fall back to another selector.
    pub selection: Option<SliceCaptureSelection>,
    pub range: FrameRange,
    /// What a copy of this object carries (only `ib` excludes attachments).
    pub attachments: SliceAttachments,
}

impl ProjectDocument {
    pub fn resolve_group_object(
        &self,
        context: &SemanticContext,
        kind: SemanticTextObject,
    ) -> Result<SemanticObjectSelection, EditError> {
        super::planner::validate_context(self, context)?;
        if kind.is_beat() {
            let beat = match &context.selected_child {
                Some(selected) => selected.clone(),
                None => self.beat_at(&context.parent, context.cursor)?,
            };
            let object = SemanticObjectSelection { kind, group: beat };
            self.resolve_object_selection(&context.parent, &object)?;
            return Ok(object);
        }
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
        if object.kind.is_beat() {
            if !children.contains(&object.group) {
                return Err(unavailable(
                    "the selected beat object is not a direct child of the current Sequence",
                ));
            }
            let span = self.sequence_children(navigation_parent, &object.group, &object.group)?;
            return Ok(SemanticObjectTarget {
                parent: navigation_parent.clone(),
                selection: Some(SliceCaptureSelection::Child {
                    node: object.group.clone(),
                }),
                range: span.range,
                attachments: object.kind.attachments(),
            });
        }
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
            SemanticTextObject::InnerBeat | SemanticTextObject::AroundBeat => {
                unreachable!("beat objects resolved above")
            }
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
            attachments: object.kind.attachments(),
        })
    }

    /// The beat whose picture follows the boundary `cursor` in `parent`: the
    /// right-hand positive-duration child, or the final one at the scope end.
    /// Equal-time empty children are never guessed from a timestamp.
    fn beat_at(&self, parent: &NodeId, cursor: ProjectFrame) -> Result<NodeId, EditError> {
        let durations = self.durations()?;
        let mut start = self.source_splice_boundary(parent, 0)?.0;
        let mut last = None;
        for child in self.children(parent) {
            let length = durations[child].frames();
            if length == 0 {
                continue;
            }
            let end = start.checked_add(length).ok_or_else(|| {
                EditError::new(
                    EditErrorCode::TimingOverflow,
                    "beat object boundary overflow",
                )
            })?;
            if start <= cursor.0 && cursor.0 < end {
                return Ok(child.clone());
            }
            last = Some((child, end));
            start = end;
        }
        match last {
            Some((child, end)) if end == cursor.0 => Ok(child.clone()),
            _ => Err(unavailable(
                "there is no beat at the cursor; select a beat before using ib/ab",
            )),
        }
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
        if kind.is_beat() {
            // The selected beat is the object, like a group's explicit choice.
            result.selected_child = Some(selection.group.clone());
        }
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
