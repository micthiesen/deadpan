//! One old-tree timing capture and one linked structural edge change.

use std::collections::BTreeSet;

use crate::{
    AudioTimingId, Command, EditError, EditErrorCode, ExactRatio, FrameDuration, FramingError,
    MAX_AUDIO_BINDING_ENTRIES, MAX_DOCUMENT_NODES, NodeKind, PitchPolicy, ProjectDocument,
    RevisionId, SourceTrimResolution,
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

    let mut result = capture_timing(document, &resolved, timing)?;
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
        // Cutaways stay on the host content they were placed over.
        physical.cutaways =
            crate::cutaways_with_owner_prefix(&physical.cutaways, resolved.physical_prefix)
                .map_err(crate::DocumentError::from)?;
    }
    physical.kind = NodeKind::Source {
        source: resolved.after.clone(),
    };
    if resolved.physical_prefix != FrameDuration::ZERO
        && let Some(binding) = result
            .audio_bindings
            .bindings
            .get_mut(&resolved.physical_source)
    {
        *binding = binding.rebase_local(ExactRatio::integer(resolved.physical_prefix.frames()))?;
    }

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
    crate::audio_binding_lifecycle::prune(&mut result);
    Ok(result)
}

fn capture_timing(
    document: &ProjectDocument,
    resolved: &SourceTrimResolution,
    timing: &AudioTimingId,
) -> Result<ProjectDocument, EditError> {
    let mut captured =
        crate::audio_binding_lifecycle::capture_for_composite_insertion(document, timing.clone())?;
    let mut working = document.clone();
    working.audio_bindings = captured.state;
    if let Some(layout) = captured.phase_only_layout {
        working
            .audio_bindings
            .timings
            .insert(timing.clone(), layout);
    }
    let mut entries = 0usize;
    for (_, _, binding) in working.audio_bindings.owners() {
        for placement in binding.placements() {
            entries = entries
                .checked_add(placement.entry_count())
                .filter(|count| *count <= MAX_AUDIO_BINDING_ENTRIES)
                .ok_or_else(|| limit("Source trim audio binding entries"))?;
        }
    }
    if let Some(window) = resolved.target_timing_window {
        let target = BTreeSet::from([resolved.physical_source.clone()]);
        let placement = captured
            .node_placements
            .remove(&resolved.physical_source)
            .ok_or_else(|| invalid("Source trim has no captured target allocation"))?;
        crate::insert_time::composite::append_steps(
            &mut working.audio_bindings.bindings,
            [(resolved.physical_source.clone(), placement)].into(),
            &target,
            window,
            &mut entries,
        )?;
    }
    if let Some(window) = resolved.suffix_timing_window {
        let suffix = crate::insert_time::composite::shifted_owners(
            document,
            &resolved.parent,
            resolved.slot + 1,
        )?;
        crate::insert_time::composite::append_steps(
            &mut working.audio_bindings.bindings,
            captured.node_placements,
            &suffix,
            window,
            &mut entries,
        )?;
        crate::insert_time::composite::append_steps(
            &mut working.audio_bindings.gap_bindings,
            captured.gap_placements,
            &suffix,
            window,
            &mut entries,
        )?;
    }
    // Keep the captured layouts until target translation and the complete tree
    // change are installed. The caller then prunes unreferenced phase-only data.
    Ok(working)
}

fn invalid(message: &str) -> EditError {
    EditError::new(EditErrorCode::InvalidCommand, message)
}

fn limit(message: &str) -> EditError {
    EditError::new(EditErrorCode::LimitExceeded, message)
}
