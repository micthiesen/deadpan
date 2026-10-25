//! Exact structural clock placement for one currently shown picture owner.
//!
//! The canonical picture walk proves the concrete occurrence at the cursor.
//! Compiled ancestry then supplies its affine clock and cropped allocation;
//! retained provider geometry is deliberately not live ancestry.

use std::{collections::BTreeMap, ops::Range};

use deadpan_core::{
    BoundaryQueryLimits, ExactRatio, FrameRange, InstancePath, MAX_DOCUMENT_DEPTH, ProjectFrame,
    ProjectId, RevisionId,
};

use super::{CompiledKind, LookupStats, RenderPlan, upper_bound};
use crate::PlanError;

use super::picture_definition::PictureBudget;

/// One concrete node's current output clock on the root project clock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PictureOwnerMap {
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub instance: InstancePath,
    /// Structural allocation clipped by ancestor output windows. Only cursor
    /// visibility is proven by this query. Provider changes such as cutaways
    /// do not change the owner's structural clock.
    pub root_allocation: Range<ExactRatio>,
    /// `root = offset + local * scale`, before allocation clipping.
    pub owner_to_root_offset: ExactRatio,
    pub owner_to_root_scale: ExactRatio,
    /// The full compiled owner extent used by canonical framing evaluation,
    /// not its integral recipe count or its visible cropped duration.
    pub effective_duration: ExactRatio,
    /// Aggregate ancestry and canonical-walk work under the supplied budget.
    pub lookup: LookupStats,
}

impl PictureOwnerMap {
    /// Normalize a nonempty root-frame interval in the owner's full output.
    /// Both requested boundaries must be inside the exact structural allocation;
    /// fractional edges are never silently clamped or rounded.
    pub fn normalized_range(
        &self,
        range: FrameRange,
    ) -> Result<(ExactRatio, ExactRatio), PlanError> {
        let start = ExactRatio::integer(range.start().0);
        let end = ExactRatio::integer(range.end().0);
        if !start.compare(end).is_lt()
            || start.compare(self.root_allocation.start).is_lt()
            || end.compare(self.root_allocation.end).is_gt()
            || !self.owner_to_root_scale.compare_integer(0).is_gt()
            || !self.effective_duration.compare_integer(0).is_gt()
        {
            return Err(invalid("the requested range exceeds the owner allocation"));
        }
        let normalize = |root: ExactRatio| -> Result<ExactRatio, PlanError> {
            Ok(root
                .checked_sub(self.owner_to_root_offset)?
                .checked_div(self.owner_to_root_scale)?
                .checked_div(self.effective_duration)?)
        };
        let start = normalize(start)?;
        let end = normalize(end)?;
        if start.compare_integer(0).is_lt()
            || !start.compare(end).is_lt()
            || end.compare(ExactRatio::ONE).is_gt()
        {
            return Err(invalid("the mapped range exceeds the owner output"));
        }
        Ok((start, end))
    }
}

fn invalid(message: &'static str) -> PlanError {
    PlanError::InvalidPictureOccurrence(message)
}

impl RenderPlan {
    /// Resolve one shown, concrete owner without decoding or expanding plays.
    /// Limits cover the canonical picture walk, ancestry visits and all repeated
    /// Sequence/Repeat lookups. The returned allocation is structural: a range
    /// may cross a cutaway that changes the owner's provider picture.
    pub fn picture_owner_map(
        &self,
        instance: &InstancePath,
        at: ProjectFrame,
        limits: BoundaryQueryLimits,
    ) -> Result<PictureOwnerMap, PlanError> {
        if self.audio_context {
            return Err(PlanError::AudioOnlyContext);
        }
        if at.0 < 0 || at.0 >= self.duration().frames() {
            return Err(PlanError::FrameOutOfRange {
                frame: at,
                duration: self.duration(),
            });
        }
        if instance.repeats.len() > MAX_DOCUMENT_DEPTH {
            return Err(invalid("the Repeat path exceeds its depth bound"));
        }
        let mut current = *self
            .by_id
            .get(&instance.node)
            .ok_or_else(|| invalid("the owner is absent from this revision"))?;
        let effective_duration = self.nodes[current].extent;
        if !effective_duration.compare_integer(0).is_gt() {
            return Err(invalid("an empty owner has no picture allocation"));
        }

        let position = ExactRatio::new(i128::from(at.0) * 2 + 1, 2)?;
        let mut budget = PictureBudget::new(limits);
        let sample = self.walk_picture_at(self.root, position, true, &mut budget, None)?;
        let mut layers = BTreeMap::new();
        for layer in sample.framing.iter().filter(|layer| !layer.escalation) {
            if layers.insert(layer.instance.node.clone(), layer).is_some() {
                return Err(PlanError::InvalidPlan("duplicate live picture owner"));
            }
        }
        let shown = layers
            .get(&instance.node)
            .filter(|layer| &layer.instance == instance)
            .ok_or_else(|| invalid("the concrete owner is not shown at this cursor"))?;
        let shown_local = shown.local_position;
        if shown.effective_duration != effective_duration {
            return Err(PlanError::InvalidPlan("picture owner extent differs"));
        }

        let mut allocation = ExactRatio::ZERO..effective_duration;
        let mut offset = ExactRatio::ZERO;
        let mut scale = ExactRatio::ONE;
        let mut remaining = instance.repeats.len();
        let mut depth = 0;
        loop {
            budget.visit()?;
            if depth > MAX_DOCUMENT_DEPTH {
                return Err(invalid("the owner ancestry exceeds its depth bound"));
            }
            let Some(parent) = self.parents[current] else {
                break;
            };
            depth += 1;
            let parent_node = &self.nodes[parent];
            let parent_layer =
                layers
                    .get(&parent_node.inspection.id)
                    .ok_or(PlanError::InvalidPlan(
                        "shown owner has an absent parent layer",
                    ))?;
            let (edge_offset, edge_scale) = match &parent_node.kind {
                CompiledKind::Sequence { entries } => {
                    let local = parent_layer.local_position;
                    let index = upper_bound(
                        entries.len(),
                        |index| !local.compare(entries[index].end).is_lt(),
                        &mut budget,
                    )?;
                    let entry = entries
                        .get(index)
                        .ok_or(PlanError::InvalidPlan("owner Sequence lookup has no child"))?;
                    if entry.child != current {
                        return Err(invalid("the Sequence does not show this child"));
                    }
                    (entry.start, ExactRatio::ONE)
                }
                CompiledKind::Retime {
                    child,
                    start,
                    scale,
                    ..
                } => {
                    if *child != current || !scale.compare_integer(0).is_gt() {
                        return Err(invalid("the Retime does not own this child"));
                    }
                    let inverse = ExactRatio::ONE.checked_div(*scale)?;
                    (
                        ExactRatio::ZERO.checked_sub(start.checked_mul(inverse)?)?,
                        inverse,
                    )
                }
                CompiledKind::Repeat { layout, .. } => {
                    let step = remaining
                        .checked_sub(1)
                        .and_then(|index| instance.repeats.get(index))
                        .ok_or_else(|| invalid("a Repeat identity is missing"))?;
                    if step.node != parent_node.inspection.id
                        || parent_layer.instance.repeats != instance.repeats[..remaining - 1]
                    {
                        return Err(invalid("the Repeat identities are reordered or mismatched"));
                    }
                    let location = layout
                        .locate_bounded(
                            parent_layer.local_position,
                            deadpan_core::InsertionBias::Right,
                            budget.comparisons_left(),
                        )
                        .map_err(|error| {
                            if error.code == deadpan_core::DocumentErrorCode::LimitExceeded {
                                PlanError::PictureQueryLimit("comparisons")
                            } else {
                                PlanError::Document(error)
                            }
                        })?;
                    budget.repeat_comparisons(location.comparisons)?;
                    if location.play.iteration != step.iteration {
                        return Err(invalid("the Repeat identity is not shown at this cursor"));
                    }
                    let child_id = &self.nodes[current].inspection.id;
                    let edge_offset = if location.in_gap {
                        if location.play.gap_child.as_ref() != Some(child_id) {
                            return Err(invalid("the selected gap does not own this child"));
                        }
                        location.play.start.checked_add(location.play.duration)?
                    } else {
                        if &location.play.child != child_id {
                            return Err(invalid("the selected play does not own this child"));
                        }
                        location.play.start
                    };
                    remaining -= 1;
                    (edge_offset, ExactRatio::ONE)
                }
                CompiledKind::Source { .. } | CompiledKind::Hold { .. } => {
                    return Err(invalid("provider history is not live owner ancestry"));
                }
            };
            let map = |value: ExactRatio| -> Result<ExactRatio, PlanError> {
                Ok(edge_offset.checked_add(value.checked_mul(edge_scale)?)?)
            };
            offset = map(offset)?;
            scale = scale.checked_mul(edge_scale)?;
            allocation = map(allocation.start)?..map(allocation.end)?;
            allocation.start = if allocation.start.compare_integer(0).is_lt() {
                ExactRatio::ZERO
            } else {
                allocation.start
            };
            if allocation.end.compare(parent_node.extent).is_gt() {
                allocation.end = parent_node.extent;
            }
            if !allocation.start.compare(allocation.end).is_lt() {
                return Err(invalid("the owner is entirely cropped by an ancestor"));
            }
            current = parent;
        }
        if current != self.root || remaining != 0 {
            return Err(invalid(
                "the concrete owner has extra or foreign Repeat identities",
            ));
        }
        if position.compare(allocation.start).is_lt()
            || !position.compare(allocation.end).is_lt()
            || position.checked_sub(offset)?.checked_div(scale)? != shown_local
        {
            return Err(invalid(
                "the cursor differs from the owner clock allocation",
            ));
        }
        Ok(PictureOwnerMap {
            project_id: self.metadata.project_id.clone(),
            revision_id: self.metadata.revision_id.clone(),
            instance: instance.clone(),
            root_allocation: allocation,
            owner_to_root_offset: offset,
            owner_to_root_scale: scale,
            effective_duration,
            lookup: budget.lookup,
        })
    }
}

#[cfg(test)]
#[path = "picture_occurrence/tests.rs"]
mod tests;
