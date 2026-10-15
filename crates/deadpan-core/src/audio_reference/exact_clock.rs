//! Exact historical geometry. Processing operands remain in the live tree.

use super::*;

/// Timing-only capture of a derived Retime. Its integral recipe is retained
/// in `FrozenAudioNode`; this record never snapshots Preserve input or media.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenRetimeClock {
    pub extent: ExactRatio,
    pub child_start: ExactRatio,
    pub child_end: ExactRatio,
}

impl FrozenAudioLayout {
    pub fn derived_clocks(&self) -> &BTreeMap<NodeId, FrozenRetimeClock> {
        &self.derived_clocks
    }

    /// Effective allocation, never the original integral processing count.
    pub fn node_extent(&self, id: &NodeId) -> Result<ExactRatio, DocumentError> {
        let node = self
            .nodes
            .get(id)
            .ok_or_else(|| invalid("frozen extent node is missing"))?;
        Ok(self
            .index
            .extents
            .get(id)
            .copied()
            .unwrap_or_else(|| ExactRatio::integer(node.duration.frames())))
    }

    pub fn retime_clock(&self, id: &NodeId) -> Result<FrozenRetimeClock, DocumentError> {
        let node = self
            .nodes
            .get(id)
            .ok_or_else(|| invalid("frozen Retime is missing"))?;
        let FrozenAudioKind::Retime { mapping, .. } = &node.kind else {
            return Err(invalid("frozen clock owner is not a Retime"));
        };
        Ok(self
            .derived_clocks
            .get(id)
            .copied()
            .unwrap_or(FrozenRetimeClock {
                extent: ExactRatio::integer(node.duration.frames()),
                child_start: ExactRatio::integer(mapping.start().0),
                child_end: ExactRatio::integer(mapping.end().0),
            }))
    }

    pub(crate) fn require_integral_clocks(&self) -> Result<(), DocumentError> {
        if self.derived_clocks.is_empty() {
            Ok(())
        } else {
            Err(invalid(
                "retained exact clocks require an exact output consumer",
            ))
        }
    }

    /// The recipe graph has already been fully admitted. Keep its counts for
    /// intrinsic processors and independently build exact effective geometry.
    pub(super) fn index_exact_clocks(
        &self,
        index: &mut FrozenIndex,
        postorder: &[&NodeId],
    ) -> Result<(), DocumentError> {
        if self.derived_clocks.is_empty() {
            return Ok(());
        }
        if self.derived_clocks.len() > MAX_DOCUMENT_NODES {
            return Err(limit("frozen derived clock limit exceeded"));
        }
        for id in self.derived_clocks.keys() {
            self.retime_clock(id)?;
        }
        for &id in postorder {
            let node = &self.nodes[id];
            let extent = match &node.kind {
                FrozenAudioKind::Sequence { children } => {
                    children.iter().try_fold(ExactRatio::ZERO, |sum, child| {
                        index.parents.insert(child.clone(), (id.clone(), sum));
                        sum.checked_add(index.extents[child])
                    })?
                }
                FrozenAudioKind::Repeat {
                    child,
                    iterations,
                    gap_duration,
                    ..
                } => {
                    let repeat = crate::ExactRepeatLayout::compile_with_gap_overrides(
                        iterations,
                        child,
                        self.overrides.get(id),
                        ExactRatio::integer(gap_duration.frames()),
                        self.gap_overrides.get(id),
                        &index.extents,
                    )?;
                    let extent = repeat.duration();
                    index.repeats.insert(id.clone(), repeat);
                    extent
                }
                FrozenAudioKind::Retime { child, purpose, .. } => {
                    let clock = self.retime_clock(id)?;
                    if !clock.extent.compare_integer(0).is_gt()
                        || clock.child_start.compare_integer(0).is_lt()
                        || !clock.child_end.compare(clock.child_start).is_gt()
                        || clock.child_end.compare(index.extents[child]).is_gt()
                    {
                        return Err(invalid(
                            "frozen derived Retime needs a positive extent and a window inside its child",
                        ));
                    }
                    let selected = clock.child_end.checked_sub(clock.child_start)?;
                    if *purpose == RetimePurpose::Partition && selected != clock.extent {
                        return Err(invalid("frozen derived partition must retain unity timing"));
                    }
                    selected.checked_div(clock.extent)?;
                    clock.extent
                }
                _ => ExactRatio::integer(node.duration.frames()),
            };
            if extent.compare_integer(i64::MAX).is_gt() {
                return Err(TimeError::Overflow.into());
            }
            index.extents.insert(id.clone(), extent);
        }
        index.root_duration = Some(crate::derived_clock::integral(index.extents[&self.root])?);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
