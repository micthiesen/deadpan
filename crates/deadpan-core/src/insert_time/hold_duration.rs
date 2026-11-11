//! Resize an ordinary Hold. Later audio follows the current tree.

use crate::{Command, EditError, FrameDuration, NodeId, ProjectDocument, RevisionId};

pub(crate) fn apply(
    document: &ProjectDocument,
    node: &NodeId,
    duration: FrameDuration,
    allocation: &RevisionId,
) -> Result<ProjectDocument, EditError> {
    let command = Command::SetHoldDuration {
        node: node.clone(),
        duration,
    };
    // Preserve the existing recipe reducer's provider fallback and Tail clamp.
    // The candidate is unpublished; all timing capture still reads `document`.
    let mut result = document.clone();
    crate::command::reduce(&mut result, &command, allocation)?;
    let before_total = document.structural_duration()?.frames();
    let after_total = result.structural_duration()?.frames();
    if before_total != after_total {
        // Refuse a resize whose suffix would cross a Retime ancestor.
        super::composite::occurrence_owners_after(document, node)?;
    }
    // The dedicated clocked-Retime path is dispatched before this helper.
    let mut structure = None;
    crate::audio_lineage::reconcile_shared(document, &mut result, &command, &mut structure)?;
    result.marks =
        crate::marks::transform_marks_shared(document, &result, &command, &mut structure)?;
    Ok(result)
}

#[cfg(test)]
mod tests;
