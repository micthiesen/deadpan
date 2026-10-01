//! One linked range deletion, retaining the original prefix and suffix clocks.

use crate::{
    AudioTimingId, Command, EditError, FrameRange, NodeId, NodeKind, ProjectDocument,
    SplitIdentities,
};

impl ProjectDocument {
    pub fn range_deletion(
        &self,
        parent: &NodeId,
        range: FrameRange,
    ) -> Result<super::SequenceRangeEdit, EditError> {
        super::sequence_range::preflight_deletion(self, parent, range)
    }
}

pub(crate) fn apply(
    document: &ProjectDocument,
    parent: &NodeId,
    range: FrameRange,
    identities: &SplitIdentities,
    timing: &AudioTimingId,
    context: crate::command::EditContext<'_>,
) -> Result<ProjectDocument, EditError> {
    if &timing.allocation != context.allocation {
        return Err(super::invalid(
            "range deletion timing allocation must equal the new revision",
        ));
    }
    let preflight = document.range_deletion(parent, range)?;
    super::validate_identities(document, None, identities)?;
    if identities.nodes.len() < preflight.required_ids {
        return Err(super::invalid(
            "range deletion needs more Split node identities",
        ));
    }
    let total = document.duration()?.frames();
    let splits = preflight.required_ids != 0;
    let suffix_timing = if range.end().0 < total {
        Some(AudioTimingId {
            allocation: timing.allocation.clone(),
            ordinal: if splits {
                timing
                    .ordinal
                    .checked_add(1)
                    .ok_or_else(|| super::limit("range deletion needs a second timing identity"))?
            } else {
                timing.ordinal
            },
        })
    } else {
        None
    };
    let mut working = if splits {
        super::sequence_range::split_endpoints(
            document, parent, range, identities, timing, context,
        )?
    } else {
        document.clone()
    };
    let removed = super::sequence_range::selected_children(&working, parent, range)?;
    if let Some(suffix_timing) = suffix_timing {
        working = super::composite::prepare_suffix(
            &working,
            parent,
            removed.end,
            range.end(),
            total,
            &suffix_timing,
        )?;
    }
    let mut result = working.clone();
    let NodeKind::Sequence { children } =
        &mut result.nodes.get_mut(parent).expect("named parent").kind
    else {
        unreachable!()
    };
    children.drain(removed.first..removed.end);
    for child in removed.nodes {
        crate::command::remove_subtree(&mut result, &child)?;
    }
    let command = Command::DeleteRange {
        parent: parent.clone(),
        range,
        identities: identities.clone(),
        timing: timing.clone(),
    };
    crate::audio_lineage::reconcile(&working, &mut result, &command)?;
    result.marks = crate::marks::transform_marks(&working, &result, &command)?;
    crate::audio_binding_lifecycle::prune(&mut result);
    Ok(result)
}
