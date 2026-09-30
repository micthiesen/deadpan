//! Atomic linked replacement in one explicitly named ordinary Sequence.

use crate::{
    Command, EditError, EditErrorCode, FrameDuration, FrameRange, MAX_DOCUMENT_NODES, NodeId,
    NodeKind, ProjectDocument, ProjectFrame, SplitIdentities,
};

/// Structural preflight only; no identities or mutation authority are allocated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceReplacement {
    /// Global, nonempty half-open Edit interval.
    pub range: FrameRange,
    /// Original direct-child interval intersecting the selected picture time.
    pub start_index: usize,
    pub end_index: usize,
    /// Split identities only, excluding the newly inserted Source.
    pub required_ids: usize,
}

impl ProjectDocument {
    pub fn source_replacement(
        &self,
        parent: &NodeId,
        range: FrameRange,
    ) -> Result<SourceReplacement, EditError> {
        let start = self.source_splice_boundary(parent, 0)?;
        let NodeKind::Sequence { children } = &self.nodes()[parent].kind else {
            unreachable!("source_splice_boundary admitted an ordinary Sequence")
        };
        let end = self.source_splice_boundary(parent, children.len())?;
        if range.duration() == FrameDuration::ZERO || range.start() < start || range.end() > end {
            return Err(super::invalid(
                "replacement requires a nonempty range inside its named Sequence",
            ));
        }
        let durations = self.durations()?;
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
                    super::physical(self, child).map_err(|error| {
                        EditError::new(error.code, "Replace selection endpoints require a Source, ordinary Hold or supported fragment; enter the intended group for other structures")
                    })?;
                    let count = super::split_node_count(self, child)?;
                    required_ids = required_ids
                        .checked_add(count)
                        .ok_or_else(super::overflow)?;
                    if split_start && split_end {
                        // The first Split's right wrapper is always a plain
                        // Partition, so the second cut refines that wrapper.
                        let original = &self.nodes()[child];
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
        if self
            .nodes()
            .len()
            .checked_add(required_ids + 1)
            .is_none_or(|count| count > MAX_DOCUMENT_NODES)
        {
            return Err(super::limit(
                "replacement exceeds the temporary document node limit",
            ));
        }
        Ok(SourceReplacement {
            range,
            start_index: first
                .ok_or_else(|| super::invalid("replacement contains no picture time"))?,
            end_index: last
                .ok_or_else(|| super::invalid("replacement contains no picture time"))?,
            required_ids,
        })
    }
}

pub(crate) fn apply(
    document: &ProjectDocument,
    parent: &NodeId,
    range: FrameRange,
    insertion: super::source_splice::InteriorInsertion<'_>,
    mut context: crate::command::EditContext<'_>,
) -> Result<ProjectDocument, EditError> {
    let NodeKind::Source { source } = &insertion.node.kind else {
        return Err(super::invalid("replacement requires a Source leaf"));
    };
    if source.duration == FrameDuration::ZERO {
        return Err(EditError::new(
            EditErrorCode::InvalidDuration,
            "replacement Source is empty",
        ));
    }
    if &insertion.timing.allocation != context.allocation {
        return Err(super::invalid(
            "replacement timing allocation must equal the new revision",
        ));
    }
    let preflight = document.source_replacement(parent, range)?;
    super::validate_identities(document, insertion.id, insertion.identities)?;
    if insertion.identities.nodes.len() < preflight.required_ids {
        return Err(super::invalid(
            "replacement needs more Split node identities",
        ));
    }
    let total = document.duration()?.frames();
    (total - range.duration().frames())
        .checked_add(source.duration.frames())
        .ok_or_else(super::overflow)?;
    let suffix_timing = crate::AudioTimingId {
        allocation: insertion.timing.allocation.clone(),
        ordinal: insertion
            .timing
            .ordinal
            .checked_add(1)
            .ok_or_else(|| super::limit("replacement needs a second timing identity"))?,
    };
    let command = Command::ReplaceSource {
        parent: parent.clone(),
        range,
        source: source.clone(),
        id: insertion.id.clone(),
        label: insertion.node.label.clone(),
        identities: insertion.identities.clone(),
        timing: insertion.timing.clone(),
    };
    let mut working = document.clone();
    working.audio_bindings = crate::audio_binding_lifecycle::capture_unbound_audio_bindings(
        document,
        insertion.timing.clone(),
    )?;
    let mut consumed = 0usize;
    for boundary in [range.start(), range.end()] {
        if let Some((target, at)) = interior(&working, parent, boundary)? {
            let count = super::split_node_count(&working, &target)?;
            let end = consumed.checked_add(count).ok_or_else(super::overflow)?;
            let identities = SplitIdentities {
                nodes: insertion
                    .identities
                    .nodes
                    .get(consumed..end)
                    .ok_or_else(|| {
                        super::invalid("replacement split exceeded its identity budget")
                    })?
                    .to_vec(),
            };
            consumed = end;
            working = crate::split::apply(
                &working,
                &target,
                at,
                &identities,
                crate::command::EditContext {
                    allocation: context.allocation,
                    allowances: context.allowances.as_deref_mut(),
                },
            )?;
        }
    }
    let NodeKind::Sequence { children } = &working.nodes()[parent].kind else {
        unreachable!()
    };
    let durations = working.durations()?;
    let mut offset = working.source_splice_boundary(parent, 0)?.0;
    let mut removed = Vec::new();
    for (index, child) in children.iter().enumerate() {
        let next = offset
            .checked_add(durations[child].frames())
            .ok_or_else(super::overflow)?;
        // Zero-duration children at either endpoint survive. Empty groups
        // strictly inside the removed interval belong to that interval.
        if (offset < next && offset >= range.start().0 && next <= range.end().0)
            || (offset == next && offset > range.start().0 && offset < range.end().0)
        {
            removed.push((index, child.clone()));
        }
        offset = next;
    }
    let first = removed
        .first()
        .ok_or_else(|| super::invalid("replacement has no selected children"))?
        .0;
    let end = removed.last().expect("nonempty removed children").0 + 1;
    working = super::composite::prepare_suffix(
        &working,
        parent,
        end,
        range.end(),
        total,
        &suffix_timing,
    )?;
    let mut result = working.clone();
    let NodeKind::Sequence { children } =
        &mut result.nodes.get_mut(parent).expect("named parent").kind
    else {
        unreachable!()
    };
    children.splice(first..end, [insertion.id.clone()]);
    for (_, child) in removed {
        crate::command::remove_subtree(&mut result, &child)?;
    }
    result.nodes.insert(insertion.id.clone(), insertion.node);
    crate::audio_lineage::reconcile(&working, &mut result, &command)?;
    result.marks = crate::marks::transform_marks(&working, &result, &command)?;
    crate::audio_binding_lifecycle::prune(&mut result);
    Ok(result)
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
