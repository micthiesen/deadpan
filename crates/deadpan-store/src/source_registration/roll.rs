//! Receipt-bound previews for an atomic adjacent Source Roll.

use super::*;
use deadpan_core::{
    AudioEditorialEdges, NodeKind, PitchPolicy, RetimePurpose, SourceRollResolution,
    SourceRollSideResolution, SourceVideo,
};

#[derive(Debug, Serialize)]
pub struct SourceRollPreview {
    pub resolution: SourceRollResolution,
    pub edit: Option<EditTransaction>,
}

impl ProjectStore {
    pub fn preview_source_roll(
        &self,
        request: &CommandRequest,
    ) -> Result<SourceRollPreview, StoreError> {
        let transaction = self.connection.unchecked_transaction()?;
        let current = crate::read_command_snapshot(&transaction, request)?;
        let Command::RollSources {
            parent,
            left,
            right,
            delta_frames,
            ..
        } = &request.command
        else {
            return Err(invalid(
                "source roll preview requires a RollSources command",
            ));
        };
        let resolution = current.source_roll(parent, left, right, *delta_frames)?;
        let edit = if resolution.applied_delta_frames == 0 {
            crate::ensure_unused_revision(&transaction, &request.new_revision)?;
            validate_source_roll(&transaction, &current, &current, request)?;
            None
        } else {
            Some(
                crate::prepare_current_command_with_admission(
                    &transaction,
                    current,
                    request,
                    None,
                    None,
                    None,
                )?
                .edit,
            )
        };
        Ok(SourceRollPreview { resolution, edit })
    }
}

pub(crate) fn validate_source_roll(
    connection: &Connection,
    current: &ProjectDocument,
    next: &ProjectDocument,
    request: &CommandRequest,
) -> Result<(), StoreError> {
    let Command::RollSources {
        parent,
        left,
        right,
        delta_frames,
        left_wrapper,
        right_wrapper,
        timing,
    } = &request.command
    else {
        return Ok(());
    };
    if timing.allocation != request.new_revision {
        return Err(invalid(
            "source roll timing allocation must equal the new revision",
        ));
    }
    let resolution = current.source_roll(parent, left, right, *delta_frames)?;
    if resolution.applied_delta_frames == 0
        && (left_wrapper.is_some() || right_wrapper.is_some() || next != current)
    {
        return Err(invalid(
            "zero source roll must preserve the document without wrappers",
        ));
    }
    if left_wrapper.is_some() && right_wrapper.is_some() {
        return Err(invalid("source roll cannot allocate two crop wrappers"));
    }
    if current.duration()? != next.duration()? {
        return Err(invalid("source roll changed the project duration"));
    }
    for (side, wrapper, edges) in [
        (
            &resolution.left,
            left_wrapper.as_ref(),
            AudioEditorialEdges {
                start: false,
                end: true,
            },
        ),
        (
            &resolution.right,
            right_wrapper.as_ref(),
            AudioEditorialEdges {
                start: true,
                end: false,
            },
        ),
    ] {
        validate_side(connection, current, next, parent, side, wrapper, edges)?;
    }
    Ok(())
}

fn validate_side(
    connection: &Connection,
    current: &ProjectDocument,
    next: &ProjectDocument,
    parent: &NodeId,
    side: &SourceRollSideResolution,
    wrapper: Option<&NodeId>,
    edges: AudioEditorialEdges,
) -> Result<(), StoreError> {
    if side.needs_wrapper != wrapper.is_some() {
        return Err(invalid(
            "source roll requires a fresh wrapper exactly for its resolved crop",
        ));
    }
    if let Some(wrapper) = wrapper
        && current.nodes().contains_key(wrapper)
    {
        return Err(invalid("source roll wrapper identity is already present"));
    }
    let record = current
        .assets()
        .get(&side.asset)
        .ok_or_else(|| invalid("source roll asset is absent from the selected revision"))?;
    if next.assets().get(&side.asset) != Some(record) {
        return Err(invalid("source roll changes its admitted asset contract"));
    }
    let id = record
        .source_qualification
        .as_ref()
        .ok_or_else(|| invalid("source roll asset has no measured source qualification"))?;
    if id != &side.qualification {
        return Err(invalid(
            "source roll resolution differs from its asset qualification",
        ));
    }
    let receipt = read_receipt(connection, id)?
        .ok_or_else(|| invalid("source roll qualification is missing"))?;
    if receipt.asset_record(record.label.clone())? != *record {
        return Err(invalid(
            "source roll asset metadata disagrees with its qualification",
        ));
    }
    check_original_binding(connection, &receipt)?;
    let Some(NodeKind::Source { source: before }) = current
        .nodes()
        .get(&side.physical_source)
        .map(|node| &node.kind)
    else {
        return Err(invalid("source roll target has no physical Source"));
    };
    let Some(NodeKind::Source { source: after }) = next
        .nodes()
        .get(&side.physical_source)
        .map(|node| &node.kind)
    else {
        return Err(invalid("source roll candidate lost its physical Source"));
    };
    if before != &side.before || after != &side.after {
        return Err(invalid(
            "source roll physical Source differs from its resolution",
        ));
    }
    for source in [before, after] {
        let SourceVideo::Stream { asset, span } = &source.video else {
            return Err(invalid("source roll requires measured video"));
        };
        if asset != &side.asset || Some(*span) != record.video {
            return Err(invalid(
                "source roll video differs from complete measured context",
            ));
        }
        if let Some(audio) = &source.audio
            && (audio.asset != side.asset || Some(audio.span) != record.audio)
        {
            return Err(invalid(
                "source roll audio differs from complete measured context",
            ));
        }
    }
    if sequence_child_at(current, parent, side.slot)? != &side.target {
        return Err(invalid(
            "source roll target moved from its resolved Sequence slot",
        ));
    }
    let next_child = sequence_child_at(next, parent, side.slot)?;
    if let Some(wrapper) = wrapper {
        let beat = next
            .nodes()
            .get(wrapper)
            .ok_or_else(|| invalid("source roll candidate lost its wrapper"))?;
        let expected = NodeKind::Retime {
            child: side.physical_source.clone(),
            duration: side.allocation_after.duration(),
            mapping: side.allocation_after,
            pitch: PitchPolicy::FollowSpeed,
            purpose: RetimePurpose::Partition,
        };
        if next_child != wrapper
            || beat.label != current.nodes()[&side.target].label
            || beat.framing.is_some()
            || !beat.audio_treatments.is_empty()
            || beat.audio_edges != Default::default()
            || beat.audio_editorial_edges != edges
            || beat.kind != expected
        {
            return Err(invalid(
                "source roll wrapper differs from its resolved crop",
            ));
        }
    } else {
        if next_child != &side.target {
            return Err(invalid(
                "source roll unexpectedly replaced its selected child",
            ));
        }
        if side.target != side.physical_source {
            let Some(NodeKind::Retime {
                duration, mapping, ..
            }) = next.nodes().get(&side.target).map(|node| &node.kind)
            else {
                return Err(invalid("source roll candidate lost its retained Partition"));
            };
            if *duration != side.allocation_after.duration() || *mapping != side.allocation_after {
                return Err(invalid(
                    "source roll candidate has the wrong Partition allocation",
                ));
            }
        }
    }
    Ok(())
}

fn sequence_child_at<'a>(
    document: &'a ProjectDocument,
    parent: &NodeId,
    slot: usize,
) -> Result<&'a NodeId, StoreError> {
    let Some(NodeKind::Sequence { children }) = document.nodes().get(parent).map(|node| &node.kind)
    else {
        return Err(invalid("source roll parent is not an ordinary Sequence"));
    };
    children
        .get(slot)
        .ok_or_else(|| invalid("source roll Sequence slot is unavailable"))
}
