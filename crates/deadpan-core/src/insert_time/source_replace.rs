//! Atomic linked replacement in one explicitly named ordinary Sequence.

use crate::{
    Command, EditError, EditErrorCode, FrameDuration, FrameRange, NodeId, NodeKind, ProjectDocument,
};

impl ProjectDocument {
    pub fn source_replacement(
        &self,
        parent: &NodeId,
        range: FrameRange,
    ) -> Result<super::SourceReplacement, EditError> {
        super::sequence_range::preflight(self, parent, range, 1)
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
