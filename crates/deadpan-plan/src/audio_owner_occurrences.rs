//! Bounded structural enumeration of concrete node occurrences on the current
//! project clock. Unlike `audio_owners`, this ignores retained Original clocks.

use std::{collections::BTreeSet, ops::Range};

use deadpan_core::{
    AudioSample, ExactRatio, InstancePath, MIX_SAMPLE_RATE, NodeId, RepeatInstance, TimeError,
};

use super::{AudioQueryLimits, CompiledKind, RenderPlan};
use crate::{AudioBoundaryRule, AudioSampleGrid, PlanError};

/// An exact affine map from the node's local frame clock to root project frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioOwnerOccurrenceMap {
    /// Root project frame at local frame zero.
    pub root_frame_at_local_zero: ExactRatio,
    /// Root project frames advanced by one local frame.
    pub root_frames_per_local_frame: ExactRatio,
}

impl AudioOwnerOccurrenceMap {
    pub fn root_frame_at(&self, local_frame: ExactRatio) -> Result<ExactRatio, PlanError> {
        Ok(self
            .root_frame_at_local_zero
            .checked_add(local_frame.checked_mul(self.root_frames_per_local_frame)?)?)
    }
}

/// One concrete active occurrence of a structural node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioOwnerOccurrence {
    pub instance: InstancePath,
    /// Complete exact owner allocation on the current structural root clock.
    pub visible_root_frames: Range<ExactRatio>,
    pub visible_root_samples: Range<AudioSample>,
    /// Full current output interval that can be affected by this occurrence.
    /// The outermost enclosing nonunity Preserve stage owns this interval,
    /// clipped by any selections above that stage.
    pub influence_root_frames: Range<ExactRatio>,
    pub influence_root_samples: Range<AudioSample>,
    pub map: AudioOwnerOccurrenceMap,
}

#[derive(Debug, Clone)]
pub struct AudioOwnerOccurrences {
    samples: Range<AudioSample>,
    occurrences: Vec<AudioOwnerOccurrence>,
    work: usize,
}

impl AudioOwnerOccurrences {
    pub fn samples(&self) -> Range<AudioSample> {
        self.samples.clone()
    }
    pub fn occurrences(&self) -> &[AudioOwnerOccurrence] {
        &self.occurrences
    }
    pub fn work(&self) -> usize {
        self.work
    }
}

impl AudioOwnerOccurrence {
    pub fn instance(&self) -> &InstancePath {
        &self.instance
    }
}

struct Query<'plan> {
    plan: &'plan RenderPlan,
    target: &'plan NodeId,
    ancestry: BTreeSet<usize>,
    requested_samples: Range<AudioSample>,
    requested_frames: Range<ExactRatio>,
    grid: AudioSampleGrid<AudioSample>,
    limits: AudioQueryLimits,
    work: usize,
    results: Vec<AudioOwnerOccurrence>,
}

impl RenderPlan {
    /// Enumerate concrete active occurrences of `owner` intersecting a bounded
    /// root-clock sample window. Structural traversal ignores retained Original
    /// bindings, never invents a default Repeat gap, and visits only Sequence
    /// entries and Repeat plays intersecting the window or Preserve influence.
    pub fn audio_owner_occurrences(
        &self,
        owner: &NodeId,
        samples: Range<AudioSample>,
        limits: AudioQueryLimits,
    ) -> Result<AudioOwnerOccurrences, PlanError> {
        limits.validate()?;
        if samples.start.0 < 0
            || samples.end < samples.start
            || samples.end > self.audio_duration()?
        {
            return Err(PlanError::AudioRangeOutOfRange);
        }
        let target =
            self.by_id
                .get(owner)
                .copied()
                .ok_or(PlanError::InvalidAudioSourceOccurrence(
                    "source occurrence owner is absent",
                ))?;
        let rate = self.metadata.presentation_basis.frame_rate;
        let grid = AudioSampleGrid::<AudioSample>::new(
            ExactRatio::ZERO,
            ExactRatio::new(
                i128::from(rate.numerator()),
                i128::from(MIX_SAMPLE_RATE) * i128::from(rate.denominator()),
            )?,
            AudioBoundaryRule::RoundEven,
        )?;
        if samples.is_empty() {
            return Ok(AudioOwnerOccurrences {
                samples,
                occurrences: Vec::new(),
                work: 0,
            });
        }
        let requested_frames = grid.at(samples.start)?..grid.at(samples.end)?;
        let mut ancestry = BTreeSet::new();
        let mut current = Some(target);
        while let Some(index) = current {
            if ancestry.len() >= limits.maximum_work
                || ancestry.len() > deadpan_core::MAX_DOCUMENT_DEPTH
            {
                return Err(PlanError::AudioQueryLimit("owner occurrence ancestry"));
            }
            ancestry.insert(index);
            current = self.parents[index];
        }
        let work = ancestry.len();
        let mut query = Query {
            plan: self,
            target: owner,
            ancestry,
            requested_samples: samples.clone(),
            requested_frames: requested_frames.clone(),
            grid,
            limits,
            work,
            results: Vec::new(),
        };
        query.visit(
            self.root,
            ExactRatio::ZERO,
            ExactRatio::ONE,
            ExactRatio::ZERO..ExactRatio::integer(self.duration().frames()),
            requested_frames,
            Vec::new(),
            None,
            0,
        )?;
        Ok(AudioOwnerOccurrences {
            samples,
            occurrences: query.results,
            work: query.work,
        })
    }
}

impl Query<'_> {
    #[allow(clippy::too_many_arguments)]
    fn visit(
        &mut self,
        node_index: usize,
        root_zero: ExactRatio,
        root_per_local: ExactRatio,
        root_clip: Range<ExactRatio>,
        root_window: Range<ExactRatio>,
        repeats: Vec<RepeatInstance>,
        preserve_influence: Option<Range<ExactRatio>>,
        depth: usize,
    ) -> Result<(), PlanError> {
        self.spend(1 + repeats.len())?;
        if depth > deadpan_core::MAX_DOCUMENT_DEPTH {
            return Err(PlanError::AudioQueryLimit("owner occurrence depth"));
        }
        if !self.ancestry.contains(&node_index) {
            return Ok(());
        }
        let node = &self.plan.nodes[node_index];
        let extent = ExactRatio::ZERO..ExactRatio::integer(node.inspection.duration.frames());
        let root_extent = map_range(root_zero, root_per_local, extent.clone())?;
        let visible_allocation = intersect(root_clip.clone(), root_extent.clone())?;
        let visible = intersect(root_window.clone(), visible_allocation.clone())?;
        // Prune on the consuming sample grid before expanding any Preserve
        // input. Exact frame overlap may allocate no output samples at all.
        // Once an outer Preserve is active, its influence governs this test:
        // the inner owner's geometric allocation may legitimately be empty.
        if !self.output_overlaps(preserve_influence.as_ref().unwrap_or(&visible_allocation))? {
            return Ok(());
        }
        let active_influence = preserve_influence.is_some();
        if !positive(&visible)? && !active_influence {
            return Ok(());
        }
        if node.inspection.id == *self.target {
            let influence = preserve_influence.unwrap_or_else(|| visible_allocation.clone());
            if ranges_overlap(&influence, &self.requested_frames)? {
                self.push(
                    InstancePath {
                        node: node.inspection.id.clone(),
                        repeats,
                    },
                    visible_allocation,
                    influence,
                    AudioOwnerOccurrenceMap {
                        root_frame_at_local_zero: root_zero,
                        root_frames_per_local_frame: root_per_local,
                    },
                )?;
            }
            return Ok(());
        }
        let local_window = intersect(
            inverse_range(root_zero, root_per_local, root_window.clone())?,
            extent,
        )?;
        if !positive(&local_window)? {
            return Ok(());
        }
        match &node.kind {
            CompiledKind::Source { .. } | CompiledKind::Hold { .. } => {}
            CompiledKind::Sequence { entries } => {
                // Indexed lower bound skips unrelated prefixes; only entries
                // touched by this query consume traversal work.
                let mut low = 0;
                let mut high = entries.len();
                while low < high {
                    self.spend(1)?;
                    let middle = low + (high - low) / 2;
                    if ExactRatio::integer(entries[middle].end)
                        .checked_sub(local_window.start)?
                        .compare_integer(0)
                        .is_le()
                    {
                        low = middle + 1;
                    } else {
                        high = middle;
                    }
                }
                for entry in &entries[low..] {
                    if !less(ExactRatio::integer(entry.start), local_window.end)? {
                        break;
                    }
                    self.spend(1)?;
                    let child_extent =
                        ExactRatio::integer(entry.start)..ExactRatio::integer(entry.end);
                    let overlap = intersect(local_window.clone(), child_extent)?;
                    if !positive(&overlap)? {
                        if ExactRatio::integer(entry.start)
                            .checked_sub(local_window.end)?
                            .compare_integer(0)
                            .is_ge()
                        {
                            break;
                        }
                        continue;
                    }
                    self.visit(
                        entry.child,
                        root_zero.checked_add(
                            ExactRatio::integer(entry.start).checked_mul(root_per_local)?,
                        )?,
                        root_per_local,
                        intersect(
                            visible_allocation.clone(),
                            map_range(
                                root_zero,
                                root_per_local,
                                ExactRatio::integer(entry.start)..ExactRatio::integer(entry.end),
                            )?,
                        )?,
                        map_range(root_zero, root_per_local, overlap)?,
                        repeats.clone(),
                        preserve_influence.clone(),
                        depth + 1,
                    )?;
                }
            }
            CompiledKind::Retime {
                child,
                start,
                scale,
                pitch,
                ..
            } => {
                let child_zero_local = ExactRatio::ZERO.checked_sub(start.checked_div(*scale)?)?;
                let child_per_local = ExactRatio::ONE.checked_div(*scale)?;
                let child_root_zero =
                    root_zero.checked_add(child_zero_local.checked_mul(root_per_local)?)?;
                let child_root_per = root_per_local.checked_mul(child_per_local)?;
                let mut influence = preserve_influence;
                if pitch.processes(*scale == ExactRatio::ONE)
                    && influence.is_none()
                    && ranges_overlap(&root_extent, &root_window)?
                {
                    influence = Some(visible_allocation.clone());
                }
                let selected = *start
                    ..start
                        .checked_add(scale.checked_mul(ExactRatio::integer(
                            node.inspection.duration.frames(),
                        ))?)?;
                let child_local_window =
                    inverse_range(child_root_zero, child_root_per, root_window.clone())?;
                let child_extent = ExactRatio::ZERO
                    ..ExactRatio::integer(self.plan.nodes[*child].inspection.duration.frames());
                let child_visible = intersect(
                    intersect(child_local_window, selected.clone())?,
                    child_extent,
                )?;
                let preserve_active = pitch.processes(*scale == ExactRatio::ONE)
                    && ranges_overlap(&root_extent, &root_window)?;
                let child_visible = if preserve_active {
                    intersect(
                        selected.clone(),
                        ExactRatio::ZERO
                            ..ExactRatio::integer(
                                self.plan.nodes[*child].inspection.duration.frames(),
                            ),
                    )?
                } else {
                    child_visible
                };
                if positive(&child_visible)? {
                    let child_window = map_range(child_root_zero, child_root_per, child_visible)?;
                    self.visit(
                        *child,
                        child_root_zero,
                        child_root_per,
                        intersect(
                            visible_allocation.clone(),
                            map_range(
                                child_root_zero,
                                child_root_per,
                                intersect(
                                    selected.clone(),
                                    ExactRatio::ZERO
                                        ..ExactRatio::integer(
                                            self.plan.nodes[*child].inspection.duration.frames(),
                                        ),
                                )?,
                            )?,
                        )?,
                        child_window,
                        repeats,
                        influence,
                        depth + 1,
                    )?;
                }
            }
            CompiledKind::Repeat { layout, .. } => {
                // local_window is already intersected with the node extent.
                let mut cursor = local_window.start;
                let end = local_window.end;
                while less(cursor, end)? {
                    let remaining = self.limits.maximum_work - self.work;
                    if remaining == 0 {
                        return Err(PlanError::AudioQueryLimit("owner occurrence work"));
                    }
                    let location = layout
                        .locate_bounded(cursor, deadpan_core::InsertionBias::Right, remaining)
                        .map_err(|error| {
                            if error.code == deadpan_core::DocumentErrorCode::LimitExceeded {
                                PlanError::AudioQueryLimit("owner occurrence Repeat work")
                            } else {
                                error.into()
                            }
                        })?;
                    self.spend(location.comparisons)?;
                    let play = location.play;
                    let relevant = |id: &NodeId| {
                        self.plan
                            .by_id
                            .get(id)
                            .is_some_and(|index| self.ancestry.contains(index))
                    };
                    if !relevant(&play.child) && !play.gap_child.as_ref().is_some_and(relevant) {
                        let next = ExactRatio::integer(location.segment_end);
                        if !less(cursor, next)? {
                            return Err(PlanError::InvalidPlan(
                                "owner occurrence compact run did not advance",
                            ));
                        }
                        cursor = next;
                        continue;
                    }
                    let child_end = ExactRatio::integer(
                        play.start
                            .checked_add(play.duration.frames())
                            .ok_or(TimeError::Overflow)?,
                    );
                    let child_start = ExactRatio::integer(play.start);
                    let child_overlap = intersect(local_window.clone(), child_start..child_end)?;
                    if let Some(child) = self.plan.by_id.get(&play.child).copied()
                        && positive(&child_overlap)?
                    {
                        let mut child_repeats = repeats.clone();
                        child_repeats.push(RepeatInstance {
                            node: node.inspection.id.clone(),
                            iteration: play.iteration.clone(),
                        });
                        self.visit(
                            child,
                            root_zero.checked_add(child_start.checked_mul(root_per_local)?)?,
                            root_per_local,
                            intersect(
                                visible_allocation.clone(),
                                map_range(root_zero, root_per_local, child_start..child_end)?,
                            )?,
                            map_range(root_zero, root_per_local, child_overlap)?,
                            child_repeats,
                            preserve_influence.clone(),
                            depth + 1,
                        )?;
                    }
                    let gap_end =
                        child_end.checked_add(ExactRatio::integer(play.gap_after.frames()))?;
                    if let Some(gap_child) = play.gap_child
                        && play.gap_after.frames() > 0
                    {
                        let gap_start = child_end;
                        let gap_overlap = intersect(local_window.clone(), gap_start..gap_end)?;
                        if positive(&gap_overlap)? {
                            let mut gap_repeats = repeats.clone();
                            gap_repeats.push(RepeatInstance {
                                node: node.inspection.id.clone(),
                                iteration: play.iteration.clone(),
                            });
                            self.visit(
                                self.plan.by_id[&gap_child],
                                root_zero.checked_add(gap_start.checked_mul(root_per_local)?)?,
                                root_per_local,
                                intersect(
                                    visible_allocation.clone(),
                                    map_range(root_zero, root_per_local, gap_start..gap_end)?,
                                )?,
                                map_range(root_zero, root_per_local, gap_overlap)?,
                                gap_repeats,
                                preserve_influence.clone(),
                                depth + 1,
                            )?;
                        }
                    }
                    // Both branches of this play were handled once above.
                    // Right-biased lookup now advances to the next live play.
                    if !less(cursor, gap_end)? {
                        return Err(PlanError::InvalidPlan(
                            "owner occurrence Repeat did not advance",
                        ));
                    }
                    cursor = gap_end;
                }
            }
        }
        Ok(())
    }

    fn output_overlaps(&self, frames: &Range<ExactRatio>) -> Result<bool, PlanError> {
        let start = self.grid.boundary(frames.start)?;
        let end = self.grid.boundary(frames.end)?;
        Ok(start < end && start < self.requested_samples.end && self.requested_samples.start < end)
    }

    fn push(
        &mut self,
        instance: InstancePath,
        visible_frames: Range<ExactRatio>,
        influence_frames: Range<ExactRatio>,
        map: AudioOwnerOccurrenceMap,
    ) -> Result<(), PlanError> {
        let visible_samples =
            self.grid.boundary(visible_frames.start)?..self.grid.boundary(visible_frames.end)?;
        let influence_samples = self.grid.boundary(influence_frames.start)?
            ..self.grid.boundary(influence_frames.end)?;
        if influence_samples.start >= influence_samples.end
            || influence_samples.start >= self.requested_samples.end
            || influence_samples.end <= self.requested_samples.start
        {
            return Ok(());
        }
        if self.results.len() == self.limits.maximum_spans {
            return Err(PlanError::AudioQueryLimit("owner occurrence count"));
        }
        self.results.push(AudioOwnerOccurrence {
            instance,
            visible_root_frames: visible_frames,
            visible_root_samples: visible_samples,
            influence_root_frames: influence_frames,
            influence_root_samples: influence_samples,
            map,
        });
        Ok(())
    }

    fn spend(&mut self, amount: usize) -> Result<(), PlanError> {
        self.work = self
            .work
            .checked_add(amount)
            .filter(|work| *work <= self.limits.maximum_work)
            .ok_or(PlanError::AudioQueryLimit("owner occurrence work"))?;
        Ok(())
    }
}

fn intersect(
    left: Range<ExactRatio>,
    right: Range<ExactRatio>,
) -> Result<Range<ExactRatio>, PlanError> {
    let start = if less(left.start, right.start)? {
        right.start
    } else {
        left.start
    };
    let end = if less(left.end, right.end)? {
        left.end
    } else {
        right.end
    };
    Ok(start..if less(end, start)? { start } else { end })
}

fn map_range(
    origin: ExactRatio,
    scale: ExactRatio,
    local: Range<ExactRatio>,
) -> Result<Range<ExactRatio>, PlanError> {
    Ok(origin.checked_add(local.start.checked_mul(scale)?)?
        ..origin.checked_add(local.end.checked_mul(scale)?)?)
}

fn inverse_range(
    origin: ExactRatio,
    scale: ExactRatio,
    root: Range<ExactRatio>,
) -> Result<Range<ExactRatio>, PlanError> {
    Ok(root.start.checked_sub(origin)?.checked_div(scale)?
        ..root.end.checked_sub(origin)?.checked_div(scale)?)
}

fn positive(range: &Range<ExactRatio>) -> Result<bool, PlanError> {
    less(range.start, range.end)
}

fn ranges_overlap(left: &Range<ExactRatio>, right: &Range<ExactRatio>) -> Result<bool, PlanError> {
    Ok(less(left.start, right.end)? && less(right.start, left.end)?)
}

fn less(left: ExactRatio, right: ExactRatio) -> Result<bool, PlanError> {
    Ok(left.checked_sub(right)?.compare_integer(0).is_lt())
}

#[cfg(test)]
#[path = "audio_owner_occurrences/tests.rs"]
mod tests;
