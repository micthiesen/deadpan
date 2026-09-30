//! Linked deletion keeps each downstream physical owner's pre-edit sample entry.

use crate::{
    AudioTimingId, Command, EditError, FrameDuration, NodeId, NodeKind, ProjectDocument, RevisionId,
};

pub(crate) fn apply(
    document: &ProjectDocument,
    node: &NodeId,
    timing: &AudioTimingId,
    allocation: &RevisionId,
) -> Result<ProjectDocument, EditError> {
    if &timing.allocation != allocation {
        return Err(super::invalid(
            "deletion timing allocation must equal the new revision",
        ));
    }
    let parent = document.parent_of(node).ok_or_else(|| {
        super::invalid("ripple deletion requires a non-root ordinary Sequence child")
    })?;
    let NodeKind::Sequence { children } = &document.nodes()[&parent].kind else {
        return Err(super::invalid(
            "ripple deletion requires an ordinary Sequence child",
        ));
    };
    let slot = children
        .iter()
        .position(|child| child == node)
        .ok_or_else(|| super::invalid("deletion target is not a child of its parent"))?;
    // This also admits the complete ancestry. Never apply root-clock reanchors
    // inside a Repeat or Retime, even when removing an empty terminal group.
    let end = document.source_splice_boundary(&parent, slot + 1)?;
    let length = document.node_duration(node)?;
    let total = document.duration()?.frames();
    let working = if length == FrameDuration::ZERO || end.0 == total {
        // No surviving time moves. Avoid retaining an unused sampling layout.
        document.clone()
    } else {
        super::composite::prepare_suffix(document, &parent, slot + 1, end, total, timing)?
    };
    let mut result = working.clone();
    // Keep the historical reducer unchanged for exact saved-history replay.
    crate::command::reduce(
        &mut result,
        &Command::Delete { node: node.clone() },
        allocation,
    )?;
    let command = Command::DeleteRipple {
        node: node.clone(),
        timing: timing.clone(),
    };
    crate::audio_lineage::reconcile(&working, &mut result, &command)?;
    result.marks = crate::marks::transform_marks(&working, &result, &command)?;
    crate::audio_binding_lifecycle::prune(&mut result);
    Ok(result)
}
