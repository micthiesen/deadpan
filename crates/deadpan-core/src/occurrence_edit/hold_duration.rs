//! A resized owned Hold moves surviving audio allocations on their old clocks.

use crate::{
    Anchor, AnchorIndex, AnchorTarget, AudioTimingId, BoundaryAnchor, EditError, ExactRatio,
    FrameDuration, InsertionBias, InstancePath, NodeKind, ProjectDocument, RevisionId,
};

pub(super) fn prepare(
    document: &ProjectDocument,
    instance: &InstancePath,
    duration: FrameDuration,
    allocation: &RevisionId,
) -> Result<ProjectDocument, EditError> {
    // The ordinary reducer retains its historical behavior for an occurrence
    // wrapper without Repeat authority. A repeated duration edit needs the
    // same root-clock admission as occurrence pause insertion.
    if instance.repeats.is_empty() {
        return Ok(document.clone());
    }
    let NodeKind::Hold { recipe } = &document.nodes()[&instance.node].kind else {
        return Err(EditError::new(
            crate::EditErrorCode::WrongNodeKind,
            "a duration change applies to a pause (Hold)",
        ));
    };
    let affected =
        crate::insert_time::composite::occurrence_owners_after(document, &instance.node)?;
    if recipe.duration == duration {
        return Ok(document.clone());
    }
    let end = AnchorIndex::new(document)
        .map_err(|error| super::invalid(&error.to_string()))?
        .resolve_target(&AnchorTarget {
            boundary: BoundaryAnchor {
                coordinate: Anchor::Occurrence {
                    instance: instance.clone(),
                    position: ExactRatio::integer(recipe.duration.frames()),
                },
                bias: InsertionBias::Right,
            },
            occurrence: None,
        })
        .map_err(|error| super::invalid(&error.to_string()))?;
    if end.exact_frame.denominator() != 1 {
        return Err(super::invalid(
            "changing one play's Hold duration needs an integral root clock",
        ));
    }
    let total = document.duration()?.frames();
    if end.frame.0 == total {
        return Ok(document.clone());
    }
    // Isolation has already happened, without moving root time. Capture now
    // so old default and owned branches keep their concrete placements, and
    // still-unbound downstream recipes retain their pre-edit phase too.
    crate::insert_time::composite::prepare_owners(
        document,
        &affected,
        end.frame,
        total,
        &AudioTimingId {
            allocation: allocation.clone(),
            ordinal: 0,
        },
    )
}
