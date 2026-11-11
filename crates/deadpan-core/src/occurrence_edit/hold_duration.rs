//! Admission for resizing an owned Hold inside one Repeat play.

use crate::{
    Anchor, AnchorIndex, AnchorTarget, BoundaryAnchor, EditError, ExactRatio, FrameDuration,
    InsertionBias, InstancePath, NodeKind, ProjectDocument,
};

pub(super) fn prepare(
    document: &ProjectDocument,
    instance: &InstancePath,
    duration: FrameDuration,
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
    // Refuse a suffix that would cross a Retime ancestor.
    crate::insert_time::composite::occurrence_owners_after(document, &instance.node)?;
    if recipe.duration == duration {
        return Ok(document.clone());
    }
    let parents = document
        .nodes()
        .keys()
        .flat_map(|parent| {
            document
                .children(parent)
                .map(move |child| (child.clone(), parent.clone()))
        })
        .collect();
    let target = crate::ScopedNodeTarget {
        node: instance.node.clone(),
        repeats: instance
            .repeats
            .iter()
            .map(|step| crate::RepeatEditStep {
                repeat: step.node.clone(),
                branch: crate::RepeatEditBranch::Play {
                    iteration: step.iteration.clone(),
                },
            })
            .collect(),
    };
    if crate::scoped_edit::duration_target_dormant(document, &target, &parents)? {
        // The preceding Play is currently final, or an enclosing definition
        // is unused. Keep the ordinary recipe/provider/mark reduction after
        // this helper.
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
    Ok(document.clone())
}
