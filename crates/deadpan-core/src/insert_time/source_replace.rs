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
    apply_in(document, parent, range, insertion, context, None)
}

/// `ReplaceSource` in a group of one already isolated Repeat play whose start
/// there is `origin`, or at an ordinary Sequence when `origin` is absent. In
/// a play, every later owner (including later plays and gaps of each
/// enclosing Repeat) keeps its own pre-edit clock.
pub(crate) fn apply_in(
    document: &ProjectDocument,
    parent: &NodeId,
    range: FrameRange,
    insertion: super::source_splice::InteriorInsertion<'_>,
    context: crate::command::EditContext<'_>,
    origin: Option<crate::ProjectFrame>,
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
    let preflight = super::sequence_range::preflight_in(document, parent, range, 1, origin)?;
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
    let command = Command::ReplaceSource {
        parent: parent.clone(),
        range,
        source: source.clone(),
        id: insertion.id.clone(),
        label: insertion.node.label.clone(),
        identities: insertion.identities.clone(),
        timing: insertion.timing.clone(),
    };
    let working = super::sequence_range::split_endpoints_in(
        document,
        parent,
        range,
        insertion.identities,
        context,
        origin,
    )?;
    let removed = super::sequence_range::selected_children_in(&working, parent, range, origin)?;
    // Refuse a suffix that would cross a Repeat or Retime ancestor.
    if origin.is_some() {
        super::composite::occurrence_shifted_owners(&working, parent, removed.end)?;
    } else {
        super::composite::check_suffix(&working, parent, removed.end)?;
    }
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
    crate::audio_binding::prune(&mut result);
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
    if selected.range.end().0 < total {
        super::composite::check_suffix(document, parent, selected.end)?;
    }
    let working = document;
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
    crate::audio_lineage::reconcile(working, &mut result, &command)?;
    result.marks = crate::marks::transform_marks(working, &result, &command)?;
    crate::audio_binding::prune(&mut result);
    result.validate()?;
    Ok(result)
}
