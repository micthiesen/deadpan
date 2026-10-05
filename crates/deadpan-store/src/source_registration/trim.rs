//! Receipt-checked previews for atomic ripple trims of linked Sources.

use super::*;
use deadpan_core::{NodeKind, PitchPolicy, RetimePurpose, SourceTrimResolution, SourceVideo};

/// Exact edge limits and the single atomic edit, if the resolved delta changes
/// media. The resolution is descriptive; commit re-runs the revision-bound
/// command and stored qualification checks.
#[derive(Debug, Serialize)]
pub struct SourceTrimPreview {
    pub resolution: SourceTrimResolution,
    pub edit: Option<EditTransaction>,
}

impl ProjectStore {
    /// Resolve against one captured SQLite snapshot, including the stored
    /// qualification and command metadata checks for a zero-delta preview.
    pub fn preview_source_trim(
        &self,
        request: &CommandRequest,
    ) -> Result<SourceTrimPreview, StoreError> {
        let transaction = self.connection.unchecked_transaction()?;
        let current = crate::read_command_snapshot(&transaction, &self.documents, request)?;
        let Command::TrimSource {
            parent,
            node,
            edge,
            delta_frames,
            mode,
            ..
        } = &request.command
        else {
            return Err(invalid("source trim preview requires a TrimSource command"));
        };
        let resolution = current.source_trim(parent, node, *edge, *delta_frames, *mode)?;
        let edit = if resolution.applied_delta_frames == 0 {
            crate::ensure_unused_revision(&transaction, &self.documents, &request.new_revision)?;
            validate_source_trim(&transaction, &current, &current, request)?;
            None
        } else {
            Some(
                crate::prepare_current_command_with_admission(
                    &transaction,
                    &self.documents,
                    current,
                    request,
                    None,
                    None,
                    None,
                )?
                .edit,
            )
        };
        Ok(SourceTrimPreview { resolution, edit })
    }
}

/// Validate stored admission and the exact physical/crop result derived by the
/// core resolver. The request remains the authority; preview JSON does not.
pub(crate) fn validate_source_trim(
    connection: &Connection,
    current: &ProjectDocument,
    next: &ProjectDocument,
    request: &CommandRequest,
) -> Result<(), StoreError> {
    let Command::TrimSource {
        parent,
        node,
        edge,
        delta_frames,
        mode,
        wrapper,
        timing,
    } = &request.command
    else {
        return Ok(());
    };
    if timing.allocation != request.new_revision {
        return Err(invalid(
            "source trim timing allocation must equal the new revision",
        ));
    }

    let resolution = current.source_trim(parent, node, *edge, *delta_frames, *mode)?;
    if resolution.applied_delta_frames == 0 {
        if wrapper.is_some() || resolution.needs_wrapper {
            return Err(invalid(
                "a zero source trim must not allocate a wrapper identity",
            ));
        }
        if resolution.before != resolution.after
            || resolution.target_timing_window.is_some()
            || resolution.suffix_timing_window.is_some()
            || resolution.root_operation.is_some()
        {
            return Err(invalid("zero source trim resolution is not an exact no-op"));
        }
    } else if resolution.needs_wrapper != wrapper.is_some() {
        return Err(invalid(
            "source trim requires a fresh wrapper exactly when the direct Source becomes cropped",
        ));
    }
    if let Some(wrapper) = wrapper
        && current.nodes().contains_key(wrapper)
    {
        return Err(invalid("source trim wrapper identity is already present"));
    }

    let record = current
        .assets()
        .get(&resolution.asset)
        .ok_or_else(|| invalid("source trim asset is absent from the selected revision"))?;
    if next.assets().get(&resolution.asset) != Some(record) {
        return Err(invalid("source trim changes its admitted asset contract"));
    }
    let id = record
        .source_qualification
        .as_ref()
        .ok_or_else(|| invalid("source trim asset has no measured source qualification"))?;
    if id != &resolution.qualification {
        return Err(invalid(
            "source trim resolution differs from its asset qualification",
        ));
    }
    let receipt = read_receipt(connection, id)?
        .ok_or_else(|| invalid("source trim qualification is missing"))?;
    if receipt.asset_record(record.label.clone())? != *record {
        return Err(invalid(
            "source trim asset metadata disagrees with its qualification",
        ));
    }
    check_original_binding(connection, &receipt)?;

    let Some(NodeKind::Source { source: before }) = current
        .nodes()
        .get(&resolution.physical_source)
        .map(|entry| &entry.kind)
    else {
        return Err(invalid("source trim target has no physical Source"));
    };
    if before != &resolution.before {
        return Err(invalid("source trim source differs from its resolution"));
    }
    let Some(NodeKind::Source { source: after }) = next
        .nodes()
        .get(&resolution.physical_source)
        .map(|entry| &entry.kind)
    else {
        return Err(invalid("source trim candidate lost its physical Source"));
    };
    if after != &resolution.after {
        return Err(invalid(
            "source trim candidate differs from the resolved linked edit",
        ));
    }
    for source in [before, after] {
        let SourceVideo::Stream { asset, span } = &source.video else {
            return Err(invalid("source trim requires measured video"));
        };
        if asset != &resolution.asset || Some(*span) != record.video {
            return Err(invalid(
                "source trim video differs from the complete measured context",
            ));
        }
        if let Some(audio) = &source.audio
            && (audio.asset != resolution.asset || Some(audio.span) != record.audio)
        {
            return Err(invalid(
                "source trim audio differs from the complete measured context",
            ));
        }
    }

    let current_children = sequence_child_at(current, parent, resolution.slot)?;
    let next_children = sequence_child_at(next, parent, resolution.slot)?;
    if current_children != node {
        return Err(invalid(
            "source trim target moved from its resolved Sequence slot",
        ));
    }
    if resolution.needs_wrapper {
        let wrapper = wrapper
            .as_ref()
            .ok_or_else(|| invalid("source trim wrapper identity is missing"))?;
        if next_children != wrapper {
            return Err(invalid(
                "source trim wrapper is not in the resolved Sequence slot",
            ));
        }
        let Some(beat) = next.nodes().get(wrapper) else {
            return Err(invalid("source trim candidate lost its wrapper"));
        };
        let expected = deadpan_core::NodeKind::Retime {
            child: resolution.physical_source.clone(),
            duration: resolution.allocation_after.duration(),
            mapping: resolution.allocation_after,
            pitch: PitchPolicy::FollowSpeed,
            purpose: RetimePurpose::Partition,
        };
        if beat.label != current.nodes()[node].label
            || beat.framing.is_some()
            || !beat.audio_treatments.is_empty()
            || beat.audio_edges != Default::default()
            || beat.kind != expected
        {
            return Err(invalid(
                "source trim wrapper differs from its resolved crop",
            ));
        }
    } else {
        if next_children != node {
            return Err(invalid(
                "source trim unexpectedly replaced its selected child",
            ));
        }
        if node != &resolution.physical_source {
            let Some(NodeKind::Retime {
                duration, mapping, ..
            }) = next.nodes().get(node).map(|entry| &entry.kind)
            else {
                return Err(invalid("source trim candidate lost its retained Partition"));
            };
            if *mapping != resolution.allocation_after
                || *duration != resolution.allocation_after.duration()
            {
                return Err(invalid(
                    "source trim candidate has the wrong Partition allocation",
                ));
            }
        }
    }
    if resolution.applied_delta_frames == 0 && next != current {
        return Err(invalid("zero source trim preview changed the document"));
    }
    Ok(())
}

fn sequence_child_at<'a>(
    document: &'a ProjectDocument,
    parent: &NodeId,
    slot: usize,
) -> Result<&'a NodeId, StoreError> {
    let Some(NodeKind::Sequence { children }) =
        document.nodes().get(parent).map(|entry| &entry.kind)
    else {
        return Err(invalid("source trim parent is not an ordinary Sequence"));
    };
    children
        .get(slot)
        .ok_or_else(|| invalid("source trim Sequence slot is unavailable"))
}
