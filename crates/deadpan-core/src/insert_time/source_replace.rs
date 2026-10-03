//! Atomic linked replacement in one explicitly named ordinary Sequence.

use crate::{
    AudioTimingId, BeatNode, Command, EditError, EditErrorCode, FrameDuration, FrameRange, NodeId,
    NodeKind, ProjectDocument, SequenceChildrenPlan,
};

impl ProjectDocument {
    pub fn source_replacement(
        &self,
        parent: &NodeId,
        range: FrameRange,
    ) -> Result<super::SourceReplacement, EditError> {
        super::sequence_range::preflight(self, parent, range, 1)
    }

    /// Resolve an exact direct-child replacement, including empty boundary
    /// owners. No endpoint Split or picture-time selection is inferred.
    pub fn source_children_replacement(
        &self,
        parent: &NodeId,
        first: &NodeId,
        last: &NodeId,
    ) -> Result<SequenceChildrenPlan, EditError> {
        // At least one selected owner is removed before the new Source is
        // installed. Unlike endpoint splitting, this cannot grow the node set.
        self.sequence_children(parent, first, last)
    }
}

pub(crate) fn apply(
    document: &ProjectDocument,
    parent: &NodeId,
    range: FrameRange,
    insertion: super::source_splice::InteriorInsertion<'_>,
    context: crate::command::EditContext<'_>,
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
    super::validate_identities(document, Some(insertion.id), insertion.identities)?;
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
    let mut working = super::sequence_range::split_endpoints(
        document,
        parent,
        range,
        insertion.identities,
        insertion.timing,
        context,
    )?;
    let removed = super::sequence_range::selected_children(&working, parent, range)?;
    working = super::composite::prepare_suffix(
        &working,
        parent,
        removed.end,
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
    children.splice(removed.first..removed.end, [insertion.id.clone()]);
    for child in removed.nodes {
        crate::command::remove_subtree(&mut result, &child)?;
    }
    result.nodes.insert(insertion.id.clone(), insertion.node);
    crate::audio_lineage::reconcile(&working, &mut result, &command)?;
    result.marks = crate::marks::transform_marks(&working, &result, &command)?;
    crate::audio_binding_lifecycle::prune(&mut result);
    Ok(result)
}

pub(crate) fn apply_children(
    document: &ProjectDocument,
    parent: &NodeId,
    (first, last): (&NodeId, &NodeId),
    node: BeatNode,
    id: &NodeId,
    timing: &AudioTimingId,
    context: crate::command::EditContext<'_>,
) -> Result<ProjectDocument, EditError> {
    let NodeKind::Source { source } = &node.kind else {
        return Err(super::invalid("replacement requires a Source leaf"));
    };
    if source.duration == FrameDuration::ZERO {
        return Err(EditError::new(
            EditErrorCode::InvalidDuration,
            "replacement Source is empty",
        ));
    }
    if &timing.allocation != context.allocation {
        return Err(super::invalid(
            "replacement timing allocation must equal the new revision",
        ));
    }
    let selected = document.source_children_replacement(parent, first, last)?;
    super::validate_identities(document, Some(id), &crate::SplitIdentities::default())?;
    let total = document.duration()?.frames();
    total
        .checked_sub(selected.range.duration().frames())
        .and_then(|value| value.checked_add(source.duration.frames()))
        .ok_or_else(super::overflow)?;
    let command = Command::ReplaceSourceChildren {
        parent: parent.clone(),
        first: first.clone(),
        last: last.clone(),
        source: source.clone(),
        id: id.clone(),
        label: node.label.clone(),
        timing: timing.clone(),
    };
    // The suffix samples still belong to the old tree when their entries are
    // captured. No intermediate deleted clock or separate insert is observed.
    let working = if selected.range.end().0 < total {
        super::composite::prepare_suffix(
            document,
            parent,
            selected.end,
            selected.range.end(),
            total,
            timing,
        )?
    } else {
        document.clone()
    };
    let mut result = working.clone();
    let NodeKind::Sequence { children } = &mut result.nodes.get_mut(parent).unwrap().kind else {
        unreachable!("exact child query admitted a Sequence")
    };
    let removed: Vec<_> = children
        .splice(selected.first..selected.end, [id.clone()])
        .collect();
    for old in removed {
        crate::command::remove_subtree(&mut result, &old)?;
    }
    result.nodes.insert(id.clone(), node);
    crate::audio_lineage::reconcile(&working, &mut result, &command)?;
    result.marks = crate::marks::transform_marks(&working, &result, &command)?;
    crate::audio_binding_lifecycle::prune(&mut result);
    result.validate()?;
    Ok(result)
}
