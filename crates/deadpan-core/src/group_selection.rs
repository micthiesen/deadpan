//! Transparent grouping in an ordinary Sequence. Partial endpoints retain their
//! complete owner contexts; no picture, sampling or root sound clock moves.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::insert_time::sequence_range;
use crate::{
    AudioTimingId, BeatNode, Command, DocumentError, EditError, EditErrorCode, FrameRange,
    MAX_DOCUMENT_NODES, NodeId, NodeKind, ProjectDocument, RevisionId, SliceCaptureSelection,
    SplitIdentities,
};

#[cfg(test)]
mod tests;

/// One neutral group and the exact endpoint Split pool, all fresh and distinct.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupSelectionIdentities {
    pub group: NodeId,
    pub split: SplitIdentities,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupSelectionPlan {
    pub range: FrameRange,
    pub required_split_ids: usize,
}

/// Group names follow ordinary document labels: at most 1024 UTF-8 bytes and
/// no NUL. Whitespace and Unicode are retained exactly, including empty labels.
pub fn validate_group_label(label: &str) -> Result<(), EditError> {
    crate::document::validate_label(label).map_err(EditError::from)
}

impl ProjectDocument {
    /// Resolve an exact child (including an empty structure) or a nonempty
    /// absolute range. Both must belong to the named ordinary Sequence.
    pub fn group_selection(
        &self,
        parent: &NodeId,
        selection: &SliceCaptureSelection,
    ) -> Result<GroupSelectionPlan, EditError> {
        match selection {
            SliceCaptureSelection::Child { node } => {
                let (_, range) = child_range(self, parent, node)?;
                if self.nodes().len() == MAX_DOCUMENT_NODES {
                    return Err(EditError::new(
                        EditErrorCode::LimitExceeded,
                        "group exceeds the document node limit",
                    ));
                }
                Ok(GroupSelectionPlan {
                    range,
                    required_split_ids: 0,
                })
            }
            SliceCaptureSelection::Range { range } => {
                let plan = sequence_range::preflight_group(self, parent, *range)?;
                Ok(GroupSelectionPlan {
                    range: *range,
                    required_split_ids: plan.required_ids,
                })
            }
            SliceCaptureSelection::Children { first, last } => {
                let selected = self.sequence_children(parent, first, last)?;
                if self.nodes().len() == MAX_DOCUMENT_NODES {
                    return Err(EditError::new(
                        EditErrorCode::LimitExceeded,
                        "group exceeds the document node limit",
                    ));
                }
                Ok(GroupSelectionPlan {
                    range: selected.range,
                    required_split_ids: 0,
                })
            }
        }
    }
}

pub(crate) fn apply(
    document: &ProjectDocument,
    command: &Command,
    context: crate::command::EditContext<'_>,
) -> Result<ProjectDocument, EditError> {
    let Command::GroupSelection {
        parent,
        selection,
        label,
        identities,
        timing,
    } = command
    else {
        return Err(invalid("expected an exact GroupSelection command"));
    };
    validate_group_label(label)?;
    let plan = document.group_selection(parent, selection)?;
    validate_identities(document, identities, plan.required_split_ids)?;
    validate_timing(document, timing, context.allocation)?;
    let mut working = document.clone();
    if plan.required_split_ids != 0 {
        // Capture before the first Split so both endpoints retain the old
        // lattice, complete Preserve context and nested Repeat birth clocks.
        working.audio_bindings = crate::capture_unbound_audio_bindings(document, timing.clone())?;
    }
    let (mut working, consumed) = sequence_range::split_prepared(
        &working,
        parent,
        &[plan.range.start(), plan.range.end()],
        &identities.split,
        context,
    )?;
    if consumed != plan.required_split_ids {
        return Err(invalid("Group Split consumption differs from preflight"));
    }
    let (first, end, children) = match selection {
        SliceCaptureSelection::Child { node } => {
            let (slot, _) = child_range(&working, parent, node)?;
            (slot, slot + 1, vec![node.clone()])
        }
        SliceCaptureSelection::Range { .. } => {
            let selected = sequence_range::selected_children(&working, parent, plan.range)?;
            (selected.first, selected.end, selected.nodes)
        }
        SliceCaptureSelection::Children { first, last } => {
            let selected = working.sequence_children(parent, first, last)?;
            let NodeKind::Sequence { children } = &working.nodes()[parent].kind else {
                unreachable!("exact child query admitted a Sequence")
            };
            (
                selected.first,
                selected.end,
                children[selected.first..selected.end].to_vec(),
            )
        }
    };
    // Split already transformed logical mark fragments. Use that complete
    // staged tree as the final-loss baseline, rather than the unsplit input.
    let before_group = working.clone();
    let NodeKind::Sequence { children: siblings } =
        &mut working.nodes.get_mut(parent).unwrap().kind
    else {
        unreachable!("group preflight admitted an ordinary Sequence")
    };
    siblings.splice(first..end, [identities.group.clone()]);
    working.nodes.insert(
        identities.group.clone(),
        BeatNode::sequence(label, children),
    );
    crate::audio_lineage::reconcile(&before_group, &mut working, command)?;
    working.marks = crate::marks::transform_marks(&before_group, &working, command)?;
    working.validate()?;
    Ok(working)
}

pub(crate) fn child_range(
    document: &ProjectDocument,
    parent: &NodeId,
    node: &NodeId,
) -> Result<(usize, FrameRange), EditError> {
    document.source_splice_boundary(parent, 0)?;
    let NodeKind::Sequence { children } = &document.nodes()[parent].kind else {
        unreachable!("boundary query admitted a Sequence")
    };
    let slot = children
        .iter()
        .position(|child| child == node)
        .ok_or_else(|| {
            EditError::new(
                EditErrorCode::SelectionUnavailable,
                "group target must be a direct child of its Sequence",
            )
        })?;
    Ok((
        slot,
        FrameRange::new(
            document.source_splice_boundary(parent, slot)?,
            document.source_splice_boundary(parent, slot + 1)?,
        )
        .map_err(DocumentError::from)?,
    ))
}

fn validate_identities(
    document: &ProjectDocument,
    identities: &GroupSelectionIdentities,
    required: usize,
) -> Result<(), EditError> {
    if identities.split.nodes.len() != required {
        return Err(invalid(
            "Group requires its exact endpoint Split identities",
        ));
    }
    let mut used: BTreeSet<_> = document.nodes().keys().collect();
    used.extend(document.audio_lineage().values().map(|value| &value.origin));
    for layout in document.audio_bindings().timings.values() {
        used.extend(layout.nodes().keys());
        used.extend(layout.audio_lineage().values().map(|value| &value.origin));
    }
    for node in std::iter::once(&identities.group).chain(&identities.split.nodes) {
        if !used.insert(node) {
            return Err(EditError::new(
                EditErrorCode::IdentityConflict,
                "Group identities must be fresh and distinct",
            ));
        }
    }
    Ok(())
}

fn validate_timing(
    document: &ProjectDocument,
    timing: &AudioTimingId,
    allocation: &RevisionId,
) -> Result<(), EditError> {
    if &timing.allocation != allocation {
        return Err(invalid(
            "Group timing allocation must equal the new revision",
        ));
    }
    if document
        .audio_bindings()
        .allocation_ids()
        .contains(allocation)
        || document
            .audio_lineage()
            .values()
            .any(|value| &value.allocation == allocation)
        || document.nodes().values().any(|node| {
            matches!(&node.kind, NodeKind::Repeat { iterations, .. }
            if iterations.segments().any(|(existing, _, _)| existing == allocation))
        })
    {
        return Err(EditError::new(
            EditErrorCode::IdentityConflict,
            "Group reuses an existing allocation",
        ));
    }
    Ok(())
}

fn invalid(message: &str) -> EditError {
    EditError::new(EditErrorCode::InvalidCommand, message)
}
