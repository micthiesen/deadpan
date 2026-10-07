//! A service-issued, reversible linked placement proposal.

use std::ops::Range;
use std::sync::Arc;

use deadpan_core::{
    AssetId, FrameDuration, FrameRange, NodeId, NodeKind, ProjectDocument, ProjectFrame, ProjectId,
    RevisionId, SemanticObjectSelection, SemanticTextObject, SliceCaptureSelection,
    SourceQualificationId,
};
use deadpan_plan::RenderPlan;

use super::{CommittedEdit, SequenceScope, Workspace, slice};

/// Native draft/change counters are nonzero and increase within a project session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProposalId {
    pub session: u64,
    pub project: ProjectId,
    pub base_revision: RevisionId,
    pub draft: u64,
    pub change: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proposal {
    pub id: ProposalId,
    pub operation: Operation,
    pub source: Source,
    pub scope: SequenceScope,
    pub parent: NodeId,
    pub destination: Destination,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Operation {
    #[default]
    Copy,
    Move,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Movement {
    pub source_parent: NodeId,
    pub source_before: FrameRange,
    pub destination_before: ProjectFrame,
    pub removal_after: ProjectFrame,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    Original {
        asset: AssetId,
        qualification: SourceQualificationId,
        ordinals: Range<u64>,
    },
    Edited {
        copied: Arc<slice::Captured>,
        /// Local refinement in the immutable source revision's global clock.
        range: FrameRange,
    },
}

impl Source {
    /// Original ordinals and historical Edit frames remain distinct source kinds.
    pub fn boundaries(&self) -> Result<Range<u64>, String> {
        match self {
            Self::Original { ordinals, .. } => Ok(ordinals.clone()),
            Self::Edited { range, .. } => Ok(u64::try_from(range.start().0)
                .map_err(|_| "Copied Edit In is negative")?
                ..u64::try_from(range.end().0).map_err(|_| "Copied Edit Out is negative")?),
        }
    }

    pub fn set_boundaries(&mut self, boundaries: Range<u64>) -> Result<(), String> {
        if boundaries.start >= boundaries.end {
            return Err("A slice must include at least one picture.".into());
        }
        match self {
            Self::Original { ordinals, .. } => *ordinals = boundaries,
            Self::Edited { range, .. } => {
                *range = FrameRange::new(
                    deadpan_core::ProjectFrame(
                        i64::try_from(boundaries.start).map_err(|_| "Copied Edit In overflows")?,
                    ),
                    deadpan_core::ProjectFrame(
                        i64::try_from(boundaries.end).map_err(|_| "Copied Edit Out overflows")?,
                    ),
                )
                .map_err(|error| error.to_string())?;
            }
        }
        Ok(())
    }

    pub fn copied_view_id(&self) -> Option<slice::CopiedViewId> {
        match self {
            Self::Original { .. } => None,
            Self::Edited { copied, range } => Some(slice::CopiedViewId {
                copy: copied.id().clone(),
                parent: copied.slice().parent().clone(),
                range: *range,
            }),
        }
    }
}

#[derive(Clone)]
pub enum PreparedMedia {
    Original,
    Edited(Arc<slice::MediaView>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Destination {
    Slot(usize),
    Interior {
        target: NodeId,
        at: FrameDuration,
    },
    /// Captured global Edit interval within the explicitly named Sequence.
    Replace {
        range: FrameRange,
    },
    /// Exact current-revision group object. `Proposal.parent` remains the
    /// captured navigation Sequence; the object supplies its edit parent.
    Object {
        selection: SemanticObjectSelection,
    },
}

#[derive(Clone, Debug)]
pub(super) struct ObjectDestination {
    pub parent: NodeId,
    pub range: FrameRange,
    pub slot: usize,
    pub endpoints: Option<(NodeId, NodeId)>,
    pub selected_group: NodeId,
    pub inner_outside: bool,
}

impl Destination {
    /// Compute the final navigation target before a fast paste commits. Object
    /// edits may change their effective parent, but that parent's Sequence path
    /// survives the replacement and can be checked in the committed refresh.
    pub(super) fn continuation(
        &self,
        workspace: &Workspace,
        scope: &SequenceScope,
        navigation_parent: &NodeId,
        inserted: &NodeId,
    ) -> Result<(SequenceScope, NodeId), String> {
        let Some(object) = self.object_target(&workspace.document, navigation_parent)? else {
            return Ok((scope.clone(), inserted.clone()));
        };
        if object.inner_outside {
            return Ok((scope.clone(), object.selected_group));
        }
        Ok((
            SequenceScope::from_historical_parent(
                &workspace.document,
                &workspace.plan,
                &object.parent,
            )?,
            inserted.clone(),
        ))
    }

    pub(super) fn object_target(
        &self,
        document: &ProjectDocument,
        navigation_parent: &NodeId,
    ) -> Result<Option<ObjectDestination>, String> {
        let Self::Object { selection } = self else {
            return Ok(None);
        };
        let target = document
            .resolve_object_selection(navigation_parent, selection)
            .map_err(|error| error.to_string())?;
        let (slot, endpoints) = match target.selection {
            Some(SliceCaptureSelection::Child { node })
                if (selection.kind == SemanticTextObject::AroundGroup
                    || selection.kind.is_beat())
                    && node == selection.group
                    && target.parent != selection.group =>
            {
                let selected = document
                    .sequence_children(&target.parent, &node, &node)
                    .map_err(|error| error.to_string())?;
                if selected.range != target.range {
                    return Err("Object child range changed during placement".into());
                }
                (selected.first, Some((node.clone(), node)))
            }
            Some(SliceCaptureSelection::Children { first, last })
                if selection.kind == SemanticTextObject::InnerGroup
                    && target.parent == selection.group =>
            {
                let selected = document
                    .sequence_children(&target.parent, &first, &last)
                    .map_err(|error| error.to_string())?;
                if selected.range != target.range {
                    return Err("Group object contents changed during placement".into());
                }
                (selected.first, Some((first, last)))
            }
            Some(SliceCaptureSelection::Range { .. })
            | Some(SliceCaptureSelection::Child { .. })
            | Some(SliceCaptureSelection::Children { .. }) => {
                return Err("Group object resolved to a different structural target".into());
            }
            None => {
                if selection.kind != SemanticTextObject::InnerGroup
                    || target.parent != selection.group
                    || document.children(&selection.group).next().is_some()
                {
                    return Err("Empty group object contents changed during placement".into());
                }
                (0, None)
            }
        };
        Ok(Some(ObjectDestination {
            inner_outside: selection.kind == SemanticTextObject::InnerGroup
                && &target.parent != navigation_parent,
            selected_group: selection.group.clone(),
            parent: target.parent,
            range: target.range,
            slot,
            endpoints,
        }))
    }
}

#[derive(Clone)]
pub struct Prepared {
    /// Exact committed base admitted by Snapshot::proposed, including media
    /// capabilities. A same-revision refresh may have replaced the UI's Arc.
    pub base: Arc<Workspace>,
    pub navigation_parent: NodeId,
    pub snapshot: Arc<deadpan_playback::Snapshot>,
    pub media: PreparedMedia,
    pub plan: Arc<RenderPlan>,
    pub node: NodeId,
    /// Parent and first child of the exact final contiguous result forest.
    pub parent: NodeId,
    /// Exact first result slot for an Object target, even when picture time is
    /// positive. Equal-time siblings cannot identify this root by range alone.
    pub result_slot: Option<usize>,
    /// Exact proposed Edit interval occupied by the linked insertion.
    pub range: FrameRange,
    /// Empty structure has an exact sibling slot; its timestamp is insufficient.
    pub empty_slot: Option<usize>,
    /// The exact interval removed from the committed base, if replacing.
    pub removed: Option<FrameRange>,
    pub movement: Option<Movement>,
    /// Final navigation is independent of the temporary object edit parent.
    pub continuation_scope: SequenceScope,
    pub continuation_node: NodeId,
    pub object: Option<SemanticObjectSelection>,
}

impl Prepared {
    /// A move can retain several roots. Validate their entire final interval,
    /// without requiring a synthetic group or treating the first root as all of it.
    pub fn validate_result(&self) -> Result<(), String> {
        if let Some(selection) = &self.object {
            let target = Destination::Object {
                selection: selection.clone(),
            }
            .object_target(&self.base.document, &self.navigation_parent)?
            .ok_or("Slice object target is missing")?;
            let expected_parent = if target.inner_outside {
                &self.navigation_parent
            } else {
                &target.parent
            };
            let navigation = self
                .continuation_scope
                .resolve_document(&self.snapshot.document, &self.plan)?;
            let expected_selected = if target.inner_outside {
                &target.selected_group
            } else {
                &self.node
            };
            if self.parent != target.parent
                || self.result_slot != Some(target.slot)
                || self.removed != Some(target.range)
                || navigation.owner != expected_parent
                || &self.continuation_node != expected_selected
                || !navigation.children.contains(expected_selected)
            {
                return Err("Slice object continuation differs from its exact target".into());
            }
        }
        if let Some(index) = self.result_slot.or(self.empty_slot) {
            let document = &self.snapshot.document;
            let Some(NodeKind::Sequence { children }) =
                document.nodes().get(&self.parent).map(|node| &node.kind)
            else {
                return Err("Empty slice parent is not an ordinary Sequence".into());
            };
            if self.empty_slot != (self.range.duration() == FrameDuration::ZERO).then_some(index)
                || children.get(index) != Some(&self.node)
                || self.plan.node_duration(&self.node) != Some(self.range.duration())
                || document
                    .source_splice_boundary(&self.parent, index)
                    .map_err(|error| error.to_string())?
                    != self.range.start()
                || self.movement.is_some()
                || (self.object.is_none() && self.removed.is_some())
            {
                return Err("Slice differs from its exact retained result slot".into());
            }
            let navigation = self
                .continuation_scope
                .resolve_document(document, &self.plan)?;
            if !navigation.children.contains(&self.continuation_node) {
                return Err("Slice continuation does not select a direct child".into());
            }
            return Ok(());
        }
        let first = result_forest_first(
            &self.snapshot.document,
            &self.plan,
            &self.parent,
            self.range,
        )?;
        if first != self.node
            || (self.movement.is_some() && self.removed.is_some())
            || (self.movement.is_none()
                && self.plan.node_duration(&self.node) != Some(self.range.duration()))
        {
            return Err("Slice result differs from its retained forest".into());
        }
        Ok(())
    }
}

pub(super) fn result_forest_first(
    document: &ProjectDocument,
    plan: &RenderPlan,
    parent: &NodeId,
    range: FrameRange,
) -> Result<NodeId, String> {
    let Some(NodeKind::Sequence { children }) = document.nodes().get(parent).map(|node| &node.kind)
    else {
        return Err("Slice result parent is not an ordinary Sequence".into());
    };
    let mut at = document
        .source_splice_boundary(parent, 0)
        .map_err(|error| error.to_string())?
        .0;
    let mut first = None;
    let mut end = range.start().0;
    for child in children {
        let duration = plan
            .node_duration(child)
            .ok_or("Slice result child is missing from its plan")?;
        let next = at
            .checked_add(duration.frames())
            .ok_or("Slice result range overflow")?;
        if next > range.start().0 && at < range.end().0 {
            if at < range.start().0 || next > range.end().0 || at != end {
                return Err("Slice result is not a contiguous complete child range".into());
            }
            first.get_or_insert_with(|| child.clone());
            end = next;
        }
        at = next;
        if at >= range.end().0 {
            break;
        }
    }
    if end != range.end().0 || range.start() >= range.end() {
        return Err("Slice result does not span its retained range".into());
    }
    first.ok_or_else(|| "Slice result has no first child".into())
}

#[derive(Clone)]
pub struct ProposalUpdate {
    pub id: ProposalId,
    /// Valid source endpoints remain available when destination preflight fails.
    pub source_view: Option<slice::SourceViewUpdate>,
    pub result: Result<Arc<Prepared>, String>,
}

/// A successful durable receipt survives a subsequent preview refresh failure.
#[derive(Clone, Debug)]
pub struct SpliceCommitUpdate {
    pub id: ProposalId,
    pub result: Result<CommittedEdit, String>,
}
