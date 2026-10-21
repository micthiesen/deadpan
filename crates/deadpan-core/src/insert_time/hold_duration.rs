//! Resize an ordinary Hold while retaining every surviving allocation's clock.

use std::collections::{BTreeMap, BTreeSet};

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
    let affected = if before_total != after_total {
        Some(super::composite::occurrence_owners_after(document, node)?)
    } else if matches!(&document.nodes()[node].kind, NodeKind::Hold { recipe } if recipe.duration != duration)
    {
        fixed_window_owners(document, node)?
    } else {
        None
    };
    if let Some(mut affected) = affected {
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
            // old entry without enumerating plays. Fixed-window interior edits
            // use each owner's own clock, including processing inputs. Unmoved
            // entries reanchor to themselves; moved entries retain old phase.
            //
            // Attached sounds share this leaf's timing identity at restoration,
            // so retain the complete old layout until that restore has run.
            let working = crate::command_work::with_scoped_timing_captures(false, || {
                let timing = AudioTimingId {
                    allocation: allocation.clone(),
                    ordinal: 0,
                };
                if before_total == after_total {
                    super::composite::prepare_hold_duration_owners(
                        document,
                        &affected,
                        before_total,
                        &timing,
                    )
                } else {
                    super::composite::prepare_owners(
                        document,
                        &affected,
                        ProjectFrame(0),
                        before_total,
                        &timing,
                    )
                }
            })?;
            result.audio_bindings = working.audio_bindings;
        }
    }
    // Exact no-op and empty-suffix edits need no physical timing capture.
    // The dedicated clocked-Retime path is dispatched before this helper.
    let mut structure = None;
    crate::audio_lineage::reconcile_shared(document, &mut result, &command, &mut structure)?;
    result.marks =
        crate::marks::transform_marks_shared(document, &result, &command, &mut structure)?;
    Ok(result)
}

/// A fixed Retime can hide root ripple while its child suffix still moves.
/// Collect only that interior ripple, and inspect the complete ancestor path.
/// Each collected recipe is captured on its own enclosing clock, including
/// processing inputs above affine windows.
fn fixed_window_owners(
    document: &ProjectDocument,
    owner: &NodeId,
) -> Result<Option<BTreeSet<NodeId>>, EditError> {
    let mut parents = BTreeMap::new();
    for id in document.nodes().keys() {
        for child in document.children(id) {
            parents.insert(child, id);
        }
    }
    let mut child = owner;
    let mut pending = Vec::new();
    let mut fixed_window = false;
    let mut live = true;
    for _ in 0..=crate::MAX_DOCUMENT_DEPTH {
        if child == document.root() {
            if !fixed_window || !live {
                return Ok(None);
            }
            let mut affected = BTreeSet::new();
            for root in pending {
                affected.extend(super::composite::repeat_interior_owners(document, root)?);
                if affected.len() > crate::MAX_DOCUMENT_NODES {
                    return Err(super::limit("Hold duration window traversal"));
                }
            }
            return Ok(Some(affected));
        }
        let ancestor = parents
            .get(child)
            .copied()
            .ok_or_else(|| super::invalid("Hold duration owner is outside the project"))?;
        match &document.nodes()[ancestor].kind {
            NodeKind::Retime { .. } => {
                // SetHoldDuration never changes this stage's selected window
                // or output extent. Its outer siblings and plays stay put.
                fixed_window = true;
            }
            NodeKind::Repeat {
                child: default,
                iterations,
                ..
            } => {
                // All override keys are distinct current plays in a validated
                // tree. Equal counts prove that the default has no live use,
                // without expanding the Repeat. Check above fixed windows too:
                // a dormant enclosing branch hides their entire interior.
                if child == default
                    && document.overrides().get(ancestor).is_some_and(|overrides| {
                        usize::try_from(iterations.len())
                            .is_ok_and(|plays| overrides.len() == plays)
                    })
                {
                    live = false;
                }
                if !fixed_window {
                    pending.clear();
                    pending.push(ancestor);
                }
            }
            NodeKind::Sequence { children } if !fixed_window => {
                let position = children
                    .iter()
                    .position(|candidate| candidate == child)
                    .ok_or_else(|| super::invalid("Hold duration owner lost its ancestor slot"))?;
                pending.extend(children[position + 1..].iter());
            }
            NodeKind::Sequence { .. } => {}
            NodeKind::Source { .. } | NodeKind::Hold { .. } => {
                return Err(super::invalid(
                    "Hold duration owner has a physical ancestor",
                ));
            }
        }
        child = ancestor;
    }
    Err(super::limit("Hold duration window depth"))
}

#[cfg(test)]
mod tests;
