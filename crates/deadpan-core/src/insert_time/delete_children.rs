//! Exact sibling deletion. Empty boundary children belong to the selection;
//! surviving sample entries and the root sound bus move only once.

use crate::{
    AudioTimingId, Command, EditError, FrameDuration, NodeId, NodeKind, ProjectDocument, RevisionId,
};

pub(crate) fn apply(
    document: &ProjectDocument,
    parent: &NodeId,
    first: &NodeId,
    last: &NodeId,
    timing: &AudioTimingId,
    allocation: &RevisionId,
) -> Result<ProjectDocument, EditError> {
    if &timing.allocation != allocation {
        return Err(super::invalid(
            "child deletion timing allocation must equal the new revision",
        ));
    }
    let selected = document.sequence_children(parent, first, last)?;
    let total = document.duration()?.frames();
    let working =
        if selected.range.duration() == FrameDuration::ZERO || selected.range.end().0 == total {
            document.clone()
        } else {
            super::composite::prepare_suffix(
                document,
                parent,
                selected.end,
                selected.range.end(),
                total,
                timing,
            )?
        };
    let mut result = working.clone();
    let NodeKind::Sequence { children } = &mut result.nodes.get_mut(parent).unwrap().kind else {
        unreachable!("exact child query admitted an ordinary Sequence")
    };
    let removed: Vec<_> = children.drain(selected.first..selected.end).collect();
    for child in removed {
        crate::command::remove_subtree(&mut result, &child)?;
    }
    let command = Command::DeleteChildren {
        parent: parent.clone(),
        first: first.clone(),
        last: last.clone(),
        timing: timing.clone(),
    };
    crate::audio_lineage::reconcile(&working, &mut result, &command)?;
    result.marks = crate::marks::transform_marks(&working, &result, &command)?;
    crate::audio_binding_lifecycle::prune(&mut result);
    Ok(result)
}
