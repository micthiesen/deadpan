//! One old-tree clock capture and two final allocations; no ripple reanchors.

use crate::{
    AudioEditorialEdges, Command, EditError, EditErrorCode, ExactRatio, FrameDuration,
    FramingError, MAX_DOCUMENT_NODES, NodeId, NodeKind, PitchPolicy, ProjectDocument, RevisionId,
    SourceRollSideResolution,
};

pub(crate) fn apply(
    document: &ProjectDocument,
    command: &Command,
    allocation: &RevisionId,
) -> Result<ProjectDocument, EditError> {
    let Command::RollSources {
        parent,
        left,
        right,
        delta_frames,
        left_wrapper,
        right_wrapper,
        timing,
    } = command
    else {
        return Err(invalid("expected an atomic Source roll command"));
    };
    if &timing.allocation != allocation {
        return Err(invalid(
            "Source roll timing allocation must equal the new revision",
        ));
    }
    let resolved = document.source_roll(parent, left, right, *delta_frames)?;
    if resolved.applied_delta_frames == 0 {
        return Err(invalid("Source roll resolves to no change"));
    }
    for (side, wrapper) in [
        (&resolved.left, left_wrapper),
        (&resolved.right, right_wrapper),
    ] {
        if side.needs_wrapper != wrapper.is_some() {
            return Err(invalid(
                "Source roll requires a fresh wrapper exactly for each contracting direct Source",
            ));
        }
        if wrapper
            .as_ref()
            .is_some_and(|id| document.nodes().contains_key(id))
        {
            return Err(EditError::new(
                EditErrorCode::IdentityConflict,
                "Source roll wrapper identity is already present",
            ));
        }
    }
    if left_wrapper.is_some() && left_wrapper == right_wrapper {
        return Err(EditError::new(
            EditErrorCode::IdentityConflict,
            "Source roll wrapper identities must be distinct",
        ));
    }
    let required = usize::from(left_wrapper.is_some()) + usize::from(right_wrapper.is_some());
    if document
        .nodes()
        .len()
        .checked_add(required)
        .is_none_or(|count| count > MAX_DOCUMENT_NODES)
    {
        return Err(EditError::new(
            EditErrorCode::LimitExceeded,
            "Source roll wrapper exceeds the document node limit",
        ));
    }
    let mut result = document.clone();
    result.audio_bindings =
        crate::audio_binding_lifecycle::capture_unbound_audio_bindings(document, timing.clone())?;
    install(&mut result, parent, &resolved.left, left_wrapper.as_ref())?;
    install(&mut result, parent, &resolved.right, right_wrapper.as_ref())?;
    // Literal adjacency was resolved before mutation. One call marks both sides
    // of this seam without touching the pair's outside neighbors.
    crate::source_edit::mark_edges(
        &mut result,
        parent,
        left_wrapper.as_ref().unwrap_or(left),
        AudioEditorialEdges {
            start: false,
            end: true,
        },
    )?;
    crate::audio_lineage::reconcile(document, &mut result, command)?;
    result.marks = crate::marks::transform_marks_with_source_prefix(
        document,
        &result,
        command,
        &resolved.right.physical_source,
        resolved.right.physical_prefix,
    )?;
    crate::audio_binding_lifecycle::prune(&mut result);
    if result.structural_durations()?[result.root()]
        != document.structural_durations()?[document.root()]
    {
        return Err(invalid("Source roll changed the project duration"));
    }
    Ok(result)
}

fn install(
    document: &mut ProjectDocument,
    parent: &NodeId,
    side: &SourceRollSideResolution,
    wrapper: Option<&NodeId>,
) -> Result<(), EditError> {
    let physical = document
        .nodes
        .get_mut(&side.physical_source)
        .ok_or_else(|| invalid("Source roll lost its physical owner"))?;
    if side.after.duration != side.before.duration
        && let Some(framing) = &physical.framing
    {
        physical.framing = Some(
            framing
                .prepend_owner_frames(side.physical_prefix, side.before.duration)
                .map_err(|error| {
                    EditError::new(
                        if error == FramingError::Overflow {
                            EditErrorCode::TimingOverflow
                        } else {
                            EditErrorCode::InvalidCommand
                        },
                        error.to_string(),
                    )
                })?,
        );
    }
    if side.physical_prefix != FrameDuration::ZERO {
        physical.audio_treatments = physical
            .audio_treatments
            .with_owner_prefix(side.physical_prefix)
            .map_err(crate::audio_gain::invalid)?;
    }
    physical.kind = NodeKind::Source {
        source: side.after.clone(),
    };
    if side.physical_prefix != FrameDuration::ZERO
        && let Some(binding) = document
            .audio_bindings
            .bindings
            .get_mut(&side.physical_source)
    {
        *binding = binding.rebase_local(ExactRatio::integer(side.physical_prefix.frames()))?;
    }
    if let Some(wrapper) = wrapper {
        let partition = crate::split::partition(
            &document.nodes()[&side.target].label,
            side.physical_source.clone(),
            side.allocation_after.start().0,
            side.allocation_after.end().0,
            PitchPolicy::FollowSpeed,
        )?;
        document.nodes.insert(wrapper.clone(), partition);
        let NodeKind::Sequence { children } = &mut document.nodes.get_mut(parent).unwrap().kind
        else {
            unreachable!()
        };
        children[side.slot] = wrapper.clone();
    } else if side.target != side.physical_source {
        let NodeKind::Retime {
            duration, mapping, ..
        } = &mut document.nodes.get_mut(&side.target).unwrap().kind
        else {
            unreachable!()
        };
        *mapping = side.allocation_after;
        *duration = mapping.duration();
    }
    Ok(())
}

fn invalid(message: &str) -> EditError {
    EditError::new(EditErrorCode::InvalidCommand, message)
}
