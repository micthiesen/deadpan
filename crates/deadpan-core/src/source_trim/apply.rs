//! One linked structural edge change.

use crate::{
    Command, EditError, EditErrorCode, FrameDuration, FramingError, MAX_DOCUMENT_NODES, NodeKind,
    PitchPolicy, ProjectDocument, RevisionId,
};

pub(crate) fn apply(
    document: &ProjectDocument,
    command: &Command,
    allocation: &RevisionId,
) -> Result<ProjectDocument, EditError> {
    let Command::TrimSource {
        parent,
        node,
        edge,
        delta_frames,
        mode,
        wrapper,
        timing,
    } = command
    else {
        return Err(invalid("expected an atomic Source trim command"));
    };
    if &timing.allocation != allocation {
        return Err(invalid(
            "Source trim timing allocation must equal the new revision",
        ));
    }
    let resolved = document.source_trim(parent, node, *edge, *delta_frames, *mode)?;
    if resolved.applied_delta_frames == 0 {
        return Err(invalid("Source trim resolves to no change"));
    }
    if resolved.needs_wrapper != wrapper.is_some() {
        return Err(invalid(
            "Source trim requires a fresh wrapper exactly when a direct Source becomes cropped",
        ));
    }
    if let Some(wrapper) = wrapper {
        if document.nodes().contains_key(wrapper) {
            return Err(EditError::new(
                EditErrorCode::IdentityConflict,
                "Source trim wrapper identity is already present",
            ));
        }
        if document.nodes().len() >= MAX_DOCUMENT_NODES {
            return Err(limit("Source trim wrapper exceeds the document node limit"));
        }
    }

    if resolved.suffix_timing_window.is_some() {
        // Refuse a suffix that would cross a Repeat or Retime ancestor.
        crate::insert_time::composite::shifted_owners(
            document,
            &resolved.parent,
            resolved.slot + 1,
        )?;
    }
    let mut result = document.clone();
    let physical = result
        .nodes
        .get_mut(&resolved.physical_source)
        .ok_or_else(|| invalid("Source trim lost its physical owner"))?;
    if resolved.after.duration != resolved.before.duration
        && let Some(framing) = &physical.framing
    {
        physical.framing = Some(
            framing
                .prepend_owner_frames(resolved.physical_prefix, resolved.before.duration)
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
    if resolved.physical_prefix != FrameDuration::ZERO {
        physical.audio_treatments = physical
            .audio_treatments
            .with_owner_prefix(resolved.physical_prefix)
            .map_err(crate::audio_gain::invalid)?;
    }
    if resolved.physical_prefix != FrameDuration::ZERO {
        // Cutaways and captions stay on the host content they were placed over.
        physical.cutaways =
            crate::cutaways_with_owner_prefix(&physical.cutaways, resolved.physical_prefix)
                .map_err(crate::DocumentError::from)?;
        physical.captions =
            crate::captions_with_owner_prefix(&physical.captions, resolved.physical_prefix)
                .map_err(crate::DocumentError::from)?;
    }
    physical.kind = NodeKind::Source {
        source: resolved.after.clone(),
    };

    if let Some(wrapper) = wrapper {
        let label = &document.nodes()[node].label;
        let partition = crate::split::partition(
            label,
            resolved.physical_source.clone(),
            resolved.allocation_after.start().0,
            resolved.allocation_after.end().0,
            PitchPolicy::FollowSpeed,
        )?;
        result.nodes.insert(wrapper.clone(), partition);
        let NodeKind::Sequence { children } = &mut result.nodes.get_mut(parent).unwrap().kind
        else {
            unreachable!()
        };
        children[resolved.slot] = wrapper.clone();
    } else if node != &resolved.physical_source {
        let NodeKind::Retime {
            duration, mapping, ..
        } = &mut result.nodes.get_mut(node).unwrap().kind
        else {
            unreachable!()
        };
        *mapping = resolved.allocation_after;
        *duration = mapping.duration();
    }

    // A changed editorial join is distinct from the neutral crop and retained
    // raw sampling support. Mark only the moved side and its incident neighbor.
    let target = wrapper.as_ref().unwrap_or(node);
    crate::source_edit::mark_edges(
        &mut result,
        parent,
        target,
        crate::AudioEditorialEdges {
            start: resolved.edge == crate::SourceTrimEdge::In,
            end: resolved.edge == crate::SourceTrimEdge::Out,
        },
    )?;

    crate::audio_lineage::reconcile(document, &mut result, command)?;
    result.marks = crate::marks::transform_marks_with_source_prefix(
        document,
        &result,
        command,
        &resolved.physical_source,
        resolved.physical_prefix,
    )?;
    crate::audio_binding::prune(&mut result);
    Ok(result)
}

fn invalid(message: &str) -> EditError {
    EditError::new(EditErrorCode::InvalidCommand, message)
}

fn limit(message: &str) -> EditError {
    EditError::new(EditErrorCode::LimitExceeded, message)
}
