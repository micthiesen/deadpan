//! Materialize a current default gap as an independently owned Hold. The
//! copied recipe remains ordinary live data.

use crate::{
    AudioEdgePolicies, AudioTimingId, BeatNode, EditError, EditErrorCode, FrameDuration,
    IterationId, NodeId, NodeKind, ProjectDocument, RevisionId,
};

pub(crate) fn isolate(
    document: &ProjectDocument,
    repeat: &NodeId,
    after: &IterationId,
    id: &NodeId,
    timing: &AudioTimingId,
    allocation: &RevisionId,
) -> Result<ProjectDocument, EditError> {
    if document.nodes().contains_key(id) || &timing.allocation != allocation {
        return Err(invalid(
            "gap isolation requires fresh node and timing identities",
        ));
    }
    if document
        .gap_overrides()
        .get(repeat)
        .is_some_and(|entries| entries.get(after).is_some())
    {
        return Err(invalid("selected gap already owns an independent subtree"));
    }
    let beat = document
        .nodes()
        .get(repeat)
        .ok_or_else(|| invalid("gap owner is missing"))?;
    let NodeKind::Repeat {
        iterations,
        gap: Some(recipe),
        ..
    } = &beat.kind
    else {
        return Err(invalid("gap isolation requires a configured Repeat gap"));
    };
    if recipe.duration == FrameDuration::ZERO
        || iterations
            .position(after)
            .is_none_or(|position| position + 1 == iterations.len())
    {
        return Err(invalid("selected play has no following gap"));
    }
    if document.audio_bindings().timings().contains_key(timing) {
        return Err(invalid("gap isolation timing identity already exists"));
    }
    let mut result = document.clone();
    materialize(&mut result, repeat, after, id, recipe, &beat.audio_edges)?;
    // No duration, picture mapping or existing anchor coordinate changes.
    // Ordinary replacement transforms would incorrectly retire gap content.
    result.validate()?;
    Ok(result)
}

/// Install one default gap as an owned Hold in `result`. The caller validates
/// the final document.
pub(crate) fn materialize(
    result: &mut ProjectDocument,
    repeat: &NodeId,
    after: &IterationId,
    id: &NodeId,
    recipe: &crate::HoldRecipe,
    edges: &AudioEdgePolicies,
) -> Result<(), EditError> {
    let mut hold = BeatNode::hold("Gap", recipe.clone());
    hold.audio_edges = AudioEdgePolicies {
        node_start: edges.repeat_gap_start,
        node_end: edges.repeat_gap_end,
        ..Default::default()
    };
    result.nodes.insert(id.clone(), hold);
    result
        .gap_overrides
        .entry(repeat.clone())
        .or_default()
        .insert(after.clone(), id.clone());
    Ok(())
}

fn invalid(message: &str) -> EditError {
    EditError::new(EditErrorCode::InvalidCommand, message)
}
