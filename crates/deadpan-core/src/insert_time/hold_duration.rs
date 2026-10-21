//! Resize an ordinary Hold while retaining every surviving allocation's clock.

use crate::{
    AudioTimingId, Command, EditError, FrameDuration, HoldAudio, NodeId, NodeKind, ProjectDocument,
    ProjectFrame, RevisionId,
};

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
        let mut affected = super::composite::occurrence_owners_after(document, node)?;
        // Silence has no surviving sample phase to transport. Keep any older
        // bindings intact, including for a later explicit provider change, but
        // do not append clocks to every silent owner after each resize. In a
        // compound of many silent Holds that would retain quadratic silent
        // work and exhaust the binding budget without preserving any sound.
        // Sources and processing stages remain conservative; their policies
        // may expose material beyond the current active output.
        affected.retain(|owner| match &document.nodes()[owner].kind {
            NodeKind::Hold { recipe } => recipe.audio != HoldAudio::Silence,
            NodeKind::Repeat { gap, .. } => gap
                .as_ref()
                .is_some_and(|recipe| recipe.audio != HoldAudio::Silence),
            NodeKind::Source { .. } | NodeKind::Retime { .. } => true,
            NodeKind::Sequence { .. } => false,
        });
        if !affected.is_empty() {
            // A raw Hold may be a shared definition, including nested defaults
            // and owned branches. Capture each physical owner once over the old
            // root window: its compact Repeat arguments select every concrete
            // old entry without enumerating plays. Unmoved entries reanchor to
            // themselves; moved entries retain their old sample phase.
            //
            // Attached sounds share this leaf's timing identity at restoration,
            // so retain the complete old layout until that restore has run.
            let working = crate::command_work::with_scoped_timing_captures(false, || {
                super::composite::prepare_owners(
                    document,
                    &affected,
                    ProjectFrame(0),
                    before_total,
                    &AudioTimingId {
                        allocation: allocation.clone(),
                        ordinal: 0,
                    },
                )
            })?;
            result.audio_bindings = working.audio_bindings;
        }
    }
    // Same no-op, terminal and dormant-definition behavior as the old reducer:
    // without moved live output no new physical timing identities are retained.
    // Ordinary unclocked Retime windows keep their existing reducer behavior;
    // the dedicated clocked-Retime path is dispatched before this helper.
    let mut structure = None;
    crate::audio_lineage::reconcile_shared(document, &mut result, &command, &mut structure)?;
    result.marks =
        crate::marks::transform_marks_shared(document, &result, &command, &mut structure)?;
    Ok(result)
}

#[cfg(test)]
mod tests;
