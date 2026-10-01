//! Shared admission and endpoint splitting for one ordinary Sequence interval.

use crate::{
    AudioTimingId, EditError, FrameDuration, FrameRange, MAX_DOCUMENT_NODES, NodeId, NodeKind,
    ProjectDocument, ProjectFrame, SplitIdentities,
};

/// Structural preflight only; no identities or mutation authority are allocated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SequenceRangeEdit {
    /// Global, nonempty half-open Edit interval.
    pub range: FrameRange,
    /// Original direct-child interval intersecting the selected picture time.
    pub start_index: usize,
    pub end_index: usize,
    /// Split identities only, excluding inserted contents.
    pub required_ids: usize,
}

pub(crate) fn preflight(
    document: &ProjectDocument,
    parent: &NodeId,
    range: FrameRange,
    inserted_nodes: usize,
) -> Result<SequenceRangeEdit, EditError> {
    preflight_with(document, parent, range, inserted_nodes, EndpointMode::Split)
}

pub(crate) fn preflight_capture(
    document: &ProjectDocument,
    parent: &NodeId,
    range: FrameRange,
) -> Result<SequenceRangeEdit, EditError> {
    preflight_with(document, parent, range, 0, EndpointMode::Capture)
}

/// Deletion retains complete endpoint contexts through Split, just as copied
/// replacement does. Its unity Partition windows may therefore be nested.
pub(crate) fn preflight_deletion(
    document: &ProjectDocument,
    parent: &NodeId,
    range: FrameRange,
) -> Result<SequenceRangeEdit, EditError> {
    preflight_with(document, parent, range, 0, EndpointMode::SliceSplit)
}

enum EndpointMode {
    Split,
    Capture,
    SliceSplit,
}

impl ProjectDocument {
    /// Preflight endpoint Split counts and peak nodes for copied replacement.
    /// Complete selected middle composites are removed as owned units.
    pub fn slice_replacement(
        &self,
        parent: &NodeId,
        range: FrameRange,
        slice: &crate::CapturedEditSlice,
    ) -> Result<SequenceRangeEdit, EditError> {
        slice.check_destination(self)?;
        preflight_with(
            self,
            parent,
            range,
            slice.identity_requirements()?.nodes,
            EndpointMode::SliceSplit,
        )
    }
}

fn preflight_with(
    document: &ProjectDocument,
    parent: &NodeId,
    range: FrameRange,
    inserted_nodes: usize,
    mode: EndpointMode,
) -> Result<SequenceRangeEdit, EditError> {
    let start = document.source_splice_boundary(parent, 0)?;
    let NodeKind::Sequence { children } = &document.nodes()[parent].kind else {
        unreachable!("source_splice_boundary admitted an ordinary Sequence")
    };
    let end = document.source_splice_boundary(parent, children.len())?;
    if range.duration() == FrameDuration::ZERO || range.start() < start || range.end() > end {
        return Err(super::invalid(
            "range edit requires a nonempty range inside its named Sequence",
        ));
    }
    let durations = document.durations()?;
    let mut offset = start.0;
    let mut first = None;
    let mut last = None;
    let mut required_ids = 0usize;
    for (index, child) in children.iter().enumerate() {
        let next = offset
            .checked_add(durations[child].frames())
            .ok_or_else(super::overflow)?;
        if offset < range.end().0 && next > range.start().0 {
            first.get_or_insert(index);
            last = Some(index + 1);
            let split_start = offset < range.start().0;
            let split_end = next > range.end().0;
            if split_start || split_end {
                let endpoint = if matches!(mode, EndpointMode::Split) {
                    super::physical(document, child)
                } else {
                    super::slice_physical(document, child)
                };
                endpoint.map_err(|error| {
                    EditError::new(error.code, "Range endpoints require a Source, ordinary Hold or supported fragment; enter the intended group for other structures")
                })?;
                if matches!(mode, EndpointMode::Capture) {
                    offset = next;
                    continue;
                }
                let count = super::split_node_count(document, child)?;
                required_ids = required_ids
                    .checked_add(count)
                    .ok_or_else(super::overflow)?;
                if split_start && split_end {
                    // The first Split's right wrapper is always a plain
                    // Partition, so the second cut refines that wrapper.
                    let original = &document.nodes()[child];
                    let already_refined = matches!(
                        original.kind,
                        NodeKind::Retime {
                            purpose: crate::RetimePurpose::Partition,
                            ..
                        }
                    ) && original.framing.is_none()
                        && original.audio_treatments.is_empty();
                    required_ids = required_ids
                        .checked_add(count - usize::from(!already_refined))
                        .ok_or_else(super::overflow)?;
                }
            }
        }
        offset = next;
    }
    if document
        .nodes()
        .len()
        .checked_add(required_ids)
        .and_then(|count| count.checked_add(inserted_nodes))
        .is_none_or(|count| count > MAX_DOCUMENT_NODES)
    {
        return Err(super::limit(
            "range edit exceeds the temporary document node limit",
        ));
    }
    Ok(SequenceRangeEdit {
        range,
        start_index: first.ok_or_else(|| super::invalid("range contains no picture time"))?,
        end_index: last.ok_or_else(|| super::invalid("range contains no picture time"))?,
        required_ids,
    })
}

pub(crate) fn split_endpoints(
    document: &ProjectDocument,
    parent: &NodeId,
    range: FrameRange,
    identities: &SplitIdentities,
    timing: &AudioTimingId,
    mut context: crate::command::EditContext<'_>,
) -> Result<ProjectDocument, EditError> {
    let mut working = document.clone();
    // Capture sampling lattices BEFORE copying retained physical owners.
    working.audio_bindings =
        crate::audio_binding_lifecycle::capture_unbound_audio_bindings(document, timing.clone())?;
    let mut consumed = 0usize;
    for boundary in [range.start(), range.end()] {
        if let Some((target, at)) = interior(&working, parent, boundary)? {
            let count = super::split_node_count(&working, &target)?;
            let end = consumed.checked_add(count).ok_or_else(super::overflow)?;
            let split_ids = SplitIdentities {
                nodes: identities
                    .nodes
                    .get(consumed..end)
                    .ok_or_else(|| super::invalid("range split exceeded its identity budget"))?
                    .to_vec(),
            };
            consumed = end;
            working = crate::split::apply(
                &working,
                &target,
                at,
                &split_ids,
                crate::command::EditContext {
                    allocation: context.allocation,
                    allowances: context.allowances.as_deref_mut(),
                },
            )?;
        }
    }
    Ok(working)
}

pub(crate) struct SelectedChildren {
    pub first: usize,
    pub end: usize,
    pub nodes: Vec<NodeId>,
}

pub(crate) fn selected_children(
    document: &ProjectDocument,
    parent: &NodeId,
    range: FrameRange,
) -> Result<SelectedChildren, EditError> {
    let NodeKind::Sequence { children } = &document.nodes()[parent].kind else {
        unreachable!("range preflight admitted an ordinary Sequence")
    };
    let durations = document.durations()?;
    let mut offset = document.source_splice_boundary(parent, 0)?.0;
    let mut removed = Vec::new();
    let mut first = None;
    let mut end = 0;
    for (index, child) in children.iter().enumerate() {
        let next = offset
            .checked_add(durations[child].frames())
            .ok_or_else(super::overflow)?;
        // Empty groups at either endpoint survive; interior empty groups retire.
        if (offset < next && offset >= range.start().0 && next <= range.end().0)
            || (offset == next && offset > range.start().0 && offset < range.end().0)
        {
            first.get_or_insert(index);
            end = index + 1;
            removed.push(child.clone());
        }
        offset = next;
    }
    Ok(SelectedChildren {
        first: first.ok_or_else(|| super::invalid("range has no selected children"))?,
        end,
        nodes: removed,
    })
}

fn interior(
    document: &ProjectDocument,
    parent: &NodeId,
    at: ProjectFrame,
) -> Result<Option<(NodeId, FrameDuration)>, EditError> {
    let NodeKind::Sequence { children } = &document.nodes()[parent].kind else {
        unreachable!()
    };
    let durations = document.durations()?;
    let mut offset = document.source_splice_boundary(parent, 0)?.0;
    for child in children {
        let next = offset
            .checked_add(durations[child].frames())
            .ok_or_else(super::overflow)?;
        if offset < at.0 && at.0 < next {
            return Ok(Some((
                child.clone(),
                FrameDuration::new(at.0 - offset).map_err(crate::DocumentError::from)?,
            )));
        }
        offset = next;
    }
    Ok(None)
}
