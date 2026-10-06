//! Materialize a current default gap as an independently owned Hold. Only the
//! timing reference is historical; the copied recipe remains ordinary live data.

use crate::{
    AudioClockRoot, AudioEdgePolicies, AudioPlacementTemplate, AudioRecipeKind, AudioRepeatValue,
    AudioTimingId, BeatNode, EditError, EditErrorCode, FrameDuration, FrozenAudioKind,
    FrozenAudioLayout, IterationId, NodeId, NodeKind, ProjectDocument, RevisionId,
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
    capture_gap_clock(&mut result, repeat, timing)?;
    materialize(&mut result, repeat, after, id, recipe, &beat.audio_edges)?;
    // No duration, picture mapping or existing anchor coordinate changes.
    // Ordinary replacement transforms would incorrectly retire gap content.
    result.validate()?;
    Ok(result)
}

/// Bind only `repeat`'s own gap clock, when it is still implicit. Every other
/// unbound owner keeps its implicit clock: gap materialization moves nothing,
/// and binding the whole project would turn a local edit into a project-wide
/// clock change.
pub(crate) fn capture_gap_clock(
    result: &mut ProjectDocument,
    repeat: &NodeId,
    timing: &AudioTimingId,
) -> Result<(), EditError> {
    if result.audio_bindings.gap_bindings.contains_key(repeat) {
        return Ok(());
    }
    let mut captured = crate::capture_unbound_audio_bindings(result, timing.clone())?;
    let binding = captured
        .gap_bindings
        .remove(repeat)
        .ok_or_else(|| invalid("could not capture the Repeat's gap clock"))?;
    let layout = captured
        .timings
        .remove(timing)
        .ok_or_else(|| invalid("could not capture the Repeat's gap clock"))?;
    result.audio_bindings.timings.insert(timing.clone(), layout);
    result
        .audio_bindings
        .gap_bindings
        .insert(repeat.clone(), binding);
    Ok(())
}

/// Install one default gap as an owned Hold in `result`, whose binding state
/// already captures the Repeat's gap clock. Several gaps of one Repeat can share
/// one capture; the caller validates the final document.
pub(crate) fn materialize(
    result: &mut ProjectDocument,
    repeat: &NodeId,
    after: &IterationId,
    id: &NodeId,
    recipe: &crate::HoldRecipe,
    edges: &AudioEdgePolicies,
) -> Result<(), EditError> {
    let mut binding = result
        .audio_bindings
        .gap_bindings
        .get(repeat)
        .cloned()
        .ok_or_else(|| invalid("gap isolation requires a captured gap clock"))?;
    close(&mut binding.lattice, &result.audio_bindings.timings, after)?;
    if let Some(resume) = &mut binding.resume {
        for term in &mut resume.phase.terms {
            close(&mut term.placement, &result.audio_bindings.timings, after)?;
        }
    }
    for step in &mut binding.reanchors {
        if close(&mut step.placement, &result.audio_bindings.timings, after)? {
            // A born gap had discarded its old enclosing cut. Closing the
            // dispatch into a canonical gap clock must retain that decision.
            step.window = None;
        }
    }
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
    result.audio_bindings.bindings.insert(id.clone(), binding);
    Ok(())
}

/// Returns true only when closing a new gap discards an enclosing old scope.
fn close(
    placement: &mut AudioPlacementTemplate,
    timings: &std::collections::BTreeMap<AudioTimingId, FrozenAudioLayout>,
    after: &IterationId,
) -> Result<bool, EditError> {
    if placement.reference.recipe != AudioRecipeKind::RepeatGap {
        return Err(invalid("gap binding contains a non-gap timing reference"));
    }
    if !matches!(placement.gap_after, Some(AudioRepeatValue::Live { .. })) {
        return Ok(false);
    }
    let layout = &timings[&placement.reference.timing];
    let FrozenAudioKind::Repeat {
        iterations,
        gap_duration,
        ..
    } = &layout.nodes()[&placement.reference.physical].kind
    else {
        return Err(invalid("gap timing owner is not a retained Repeat"));
    };
    let survives = *gap_duration != FrameDuration::ZERO
        && iterations
            .position(after)
            .is_some_and(|position| position + 1 < iterations.len())
        && layout
            .gap_overrides()
            .get(&placement.reference.physical)
            .is_none_or(|entries| entries.get(after).is_none());
    if survives {
        placement.gap_after = Some(AudioRepeatValue::Captured {
            iteration: after.clone(),
        });
        return Ok(false);
    }
    let canonical = AudioClockRoot::GapDefinitionPointCeil {
        repeat: placement.reference.physical.clone(),
    };
    let changed_scope = placement.reference.root != canonical;
    placement.reference.root = canonical;
    placement.gap_after = None;
    placement.arguments.clear();
    placement.births.clear();
    Ok(changed_scope)
}

fn invalid(message: &str) -> EditError {
    EditError::new(EditErrorCode::InvalidCommand, message)
}
