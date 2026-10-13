//! Persisted effective clocks, separate from integral processing recipes.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    DocumentError, DocumentErrorCode, ExactRatio, ExactRepeatLayout, FrameDuration, NodeId,
    NodeKind, PitchPolicy, ProjectDocument, RetimePurpose, TimeError,
};

#[cfg(test)]
mod tests;

/// An exact allocation and child window of a structurally derived Retime.
/// The ordinary Retime fields retain its integral recipe, independently of
/// this effective clock. Neither count may be inferred by rounding the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DerivedRetimeClock {
    pub extent: ExactRatio,
    pub child_start: ExactRatio,
    pub child_end: ExactRatio,
}

fn unsupported() -> DocumentError {
    DocumentError::new(
        DocumentErrorCode::InvalidTree,
        "derived clocks require an exact clock consumer; integral editing and retained routes are not yet supported",
    )
}

pub(crate) fn integral(value: ExactRatio) -> Result<FrameDuration, DocumentError> {
    if value.denominator() != 1 {
        return Err(DocumentError::new(
            DocumentErrorCode::InvalidDuration,
            "effective duration is fractional; use the exact duration query",
        ));
    }
    Ok(FrameDuration::new(
        i64::try_from(value.numerator()).map_err(|_| TimeError::Overflow)?,
    )?)
}

impl ProjectDocument {
    pub fn derived_clocks(&self) -> &BTreeMap<NodeId, DerivedRetimeClock> {
        &self.derived_clocks
    }

    pub(crate) fn require_integral_clocks(&self) -> Result<(), DocumentError> {
        if self.derived_clocks.is_empty() {
            Ok(())
        } else {
            Err(unsupported())
        }
    }

    /// Actual allocation of every node. Source/Hold recipes remain integral;
    /// Sequence and Repeat compose exact child extents without rounding.
    pub fn exact_durations(&self) -> Result<BTreeMap<NodeId, ExactRatio>, DocumentError> {
        let recipes = self.recipe_durations_shared()?;
        self.effective_durations(&recipes)
    }

    /// Full structural validation must precede this bounded postorder pass.
    pub(crate) fn effective_durations(
        &self,
        recipes: &BTreeMap<NodeId, FrameDuration>,
    ) -> Result<BTreeMap<NodeId, ExactRatio>, DocumentError> {
        if self.derived_clocks.is_empty() {
            return Ok(recipes
                .iter()
                .map(|(id, duration)| (id.clone(), ExactRatio::integer(duration.frames())))
                .collect());
        }
        // These consumers still use integral owner clocks. Refuse at document
        // admission, including when a clock is installed through a raw patch.
        if !self.marks.is_empty()
            || !self.sounds.is_empty()
            || !self.beat_sounds.is_empty()
            || !self.sound_routes.is_empty()
            || !self.sound_allowances.is_empty()
            || !self.audio_lineage.is_empty()
            || !self.audio_bindings.is_empty()
            || self.nodes.values().any(|node| {
                matches!(node.kind, NodeKind::Retime { pitch, .. } if pitch != PitchPolicy::FollowSpeed)
            })
        {
            return Err(unsupported());
        }
        for id in self.derived_clocks.keys() {
            if !matches!(
                self.nodes.get(id).map(|node| &node.kind),
                Some(NodeKind::Retime { .. })
            ) {
                return Err(DocumentError::new(
                    DocumentErrorCode::InvalidTree,
                    "a derived clock must belong to an existing Retime",
                ));
            }
        }
        let mut order = Vec::with_capacity(self.nodes.len());
        let mut pending = vec![&self.root];
        while let Some(id) = pending.pop() {
            order.push(id);
            pending.extend(self.children(id));
        }
        let mut extents = BTreeMap::new();
        for id in order.into_iter().rev() {
            let extent = match &self.nodes[id].kind {
                NodeKind::Source { .. } | NodeKind::Hold { .. } => {
                    ExactRatio::integer(recipes[id].frames())
                }
                NodeKind::Sequence { children } => {
                    children.iter().try_fold(ExactRatio::ZERO, |sum, child| {
                        sum.checked_add(extents[child])
                    })?
                }
                NodeKind::Repeat {
                    child,
                    iterations,
                    gap,
                    ..
                } => ExactRepeatLayout::compile_with_gap_overrides(
                    iterations,
                    child,
                    self.overrides.get(id),
                    gap.as_ref().map_or(ExactRatio::ZERO, |recipe| {
                        ExactRatio::integer(recipe.duration.frames())
                    }),
                    self.gap_overrides.get(id),
                    &extents,
                )?
                .duration(),
                NodeKind::Retime {
                    child,
                    duration,
                    mapping,
                    purpose,
                    ..
                } => {
                    let clock =
                        self.derived_clocks
                            .get(id)
                            .copied()
                            .unwrap_or(DerivedRetimeClock {
                                extent: ExactRatio::integer(duration.frames()),
                                child_start: ExactRatio::integer(mapping.start().0),
                                child_end: ExactRatio::integer(mapping.end().0),
                            });
                    if !clock.extent.compare_integer(0).is_gt()
                        || clock.child_start.compare_integer(0).is_lt()
                        || !clock.child_end.compare(clock.child_start).is_gt()
                        || clock.child_end.compare(extents[child]).is_gt()
                    {
                        return Err(DocumentError::new(
                            DocumentErrorCode::SourceRangeInvalid,
                            format!(
                                "derived Retime {id} needs a positive extent and a window inside child {child}"
                            ),
                        ));
                    }
                    let selected = clock.child_end.checked_sub(clock.child_start)?;
                    if *purpose == RetimePurpose::Partition && selected != clock.extent {
                        return Err(DocumentError::new(
                            DocumentErrorCode::InvalidTree,
                            "a derived partition must retain unity timing",
                        ));
                    }
                    // Prove the actual compiled map is representable now.
                    selected.checked_div(clock.extent)?;
                    clock.extent
                }
            };
            if extent.compare_integer(i64::MAX).is_gt() {
                return Err(TimeError::Overflow.into());
            }
            extents.insert(id.clone(), extent);
        }
        integral(extents[&self.root])?;
        Ok(extents)
    }
}
