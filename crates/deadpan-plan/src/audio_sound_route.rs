//! Retained physical sample clocks for exact sound routes. A frame-only flattening
//! cannot reproduce successive rounded edit seams. Each history entry therefore
//! retains its own allocation grid, and every Keep resumes the preceding entry
//! at that entry's physical sample boundary. Recipe phase is never reconstructed
//! from the final picture coordinate or a rounded duration ratio.

use std::{marker::PhantomData, ops::Range};

use deadpan_core::{
    AudioSample, ExactRatio, SoundRoute, SoundRouteNode, SoundRouteQueryLimits,
    SoundRouteQueryStats, TimeError,
};

use crate::{
    AudioBoundaryRule, AudioQueryLimits, AudioSampleGrid, AudioSampleMap, PlanError,
    ReferenceSample, SignalSample,
};

/// One route and one physical grid per chronological node. These are borrowed
/// preparation semantics, not persisted event state or media admission. The host
/// must retain the recipe's full source/DSP context and immutable voice identity.
/// Unity route edits may move the grid origin but cannot change spacing or rule;
/// time/pitch processing remains a separate operation.
#[derive(Debug, Clone)]
pub struct AudioSoundRoute<S> {
    route: SoundRoute,
    grids: Vec<AudioSampleGrid<ReferenceSample>>,
    allocations: Vec<Range<i64>>,
    domain: PhantomData<S>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioSoundRouteSpan<S> {
    pub samples: Range<S>,
    /// Exact lookup in the complete retained recipe's frame clock. None denotes
    /// a route Gap or exhausted previous output, independently of source support,
    /// filter taps, creative edges and Hold policy.
    pub sampling: Option<AudioSampleMap<S>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioSoundRouteQuery<S> {
    pub samples: Range<S>,
    pub spans: Vec<AudioSoundRouteSpan<S>>,
    pub stats: SoundRouteQueryStats,
}

struct RawSpan {
    samples: Range<i64>,
    local_at_start: Option<ExactRatio>,
}

struct QueryWork {
    limits: AudioQueryLimits,
    stats: SoundRouteQueryStats,
    step: ExactRatio,
    spans: Vec<RawSpan>,
}

impl<S> AudioSoundRoute<S> {
    pub fn route(&self) -> &SoundRoute {
        &self.route
    }

    fn new_inner(
        route: SoundRoute,
        grids: Vec<AudioSampleGrid<ReferenceSample>>,
        rule: AudioBoundaryRule,
    ) -> Result<Self, PlanError> {
        if grids.len() != route.nodes().len() {
            return Err(PlanError::InvalidPlan(
                "sound route needs one retained sample grid per node",
            ));
        }
        let step = grids[0].frames_per_sample();
        if grids
            .iter()
            .any(|grid| grid.boundary_rule() != rule || grid.frames_per_sample() != step)
        {
            return Err(PlanError::InvalidPlan(
                "unity sound route cannot change sample spacing or boundary rule",
            ));
        }
        let allocations = grids
            .iter()
            .enumerate()
            .map(|(index, grid)| {
                let extent = route
                    .node_extent(u32::try_from(index).map_err(|_| TimeError::Overflow)?)
                    .ok_or(PlanError::InvalidPlan("sound route extent is absent"))?;
                Ok(grid.boundary(ExactRatio::ZERO)?.0..grid.boundary(extent)?.0)
            })
            .collect::<Result<Vec<_>, PlanError>>()?;
        Ok(Self {
            route,
            grids,
            allocations,
            domain: PhantomData,
        })
    }

    fn query_inner(
        &self,
        samples: Range<i64>,
        limits: AudioQueryLimits,
    ) -> Result<QueryWork, PlanError> {
        limits.validate()?;
        let root = self.route.root() as usize;
        let allocation = &self.allocations[root];
        if samples.start > samples.end
            || samples.start < allocation.start
            || samples.end > allocation.end
        {
            return Err(PlanError::AudioRangeOutOfRange);
        }
        let mut work = QueryWork {
            limits,
            stats: SoundRouteQueryStats::default(),
            step: self.grids[0].frames_per_sample(),
            spans: vec![],
        };
        self.walk(root, samples, allocation.clone(), 0, &mut work)?;
        Ok(work)
    }

    // SoundRoute admission bounds total history/map depth to 64. Only intervals
    // reached by this query recurse; compact Repeat periods are located directly.
    fn walk(
        &self,
        id: usize,
        samples: Range<i64>,
        allocation: Range<i64>,
        destination_shift: i128,
        work: &mut QueryWork,
    ) -> Result<(), PlanError> {
        if samples.is_empty() {
            return Ok(());
        }
        work.spend(1)?;
        work.stats.node_visits += 1;
        // Different rounded placements can request one sample beyond the old
        // selected output. Its audible mask is independent of full recipe/DSP
        // support. Never expose adjacent output excluded by this Window/Keep.
        if samples.start < allocation.start {
            work.push(
                samples.start..samples.end.min(allocation.start),
                destination_shift,
                None,
            )?;
        }
        let inside = samples.start.max(allocation.start)..samples.end.min(allocation.end);
        if !inside.is_empty() {
            self.walk_inside(id, inside, destination_shift, work)?;
        }
        if samples.end > allocation.end {
            work.push(
                samples.start.max(allocation.end)..samples.end,
                destination_shift,
                None,
            )?;
        }
        Ok(())
    }

    fn walk_inside(
        &self,
        id: usize,
        samples: Range<i64>,
        destination_shift: i128,
        work: &mut QueryWork,
    ) -> Result<(), PlanError> {
        let grid = &self.grids[id];
        match &self.route.nodes()[id] {
            SoundRouteNode::Recipe {} => work.push(
                samples.clone(),
                destination_shift,
                Some(grid.at(ReferenceSample(samples.start))?),
            ),
            SoundRouteNode::Window { input, selection } => {
                let input = *input as usize;
                let old_start = self.grids[input].boundary(selection.start)?.0;
                let old_end = self.grids[input].boundary(selection.end)?.0;
                let new_start = self.allocations[id].start;
                let delta = i128::from(old_start) - i128::from(new_start);
                self.walk(
                    input,
                    shifted(samples, delta)?,
                    old_start..old_end,
                    destination_shift - delta,
                    work,
                )
            }
            SoundRouteNode::Ripple { input, map } => {
                let input = *input as usize;
                let mut cursor = samples.start;
                while cursor < samples.end {
                    let remaining = work.limits.maximum_work - work.stats.work;
                    if remaining == 0 {
                        return Err(PlanError::AudioQueryLimit("sound route work"));
                    }
                    let (at, bias) = grid.probe(ReferenceSample(cursor))?;
                    let located = map.locate(
                        at,
                        bias,
                        SoundRouteQueryLimits {
                            maximum_work: remaining,
                            maximum_spans: 1,
                        },
                    )?;
                    work.spend(located.stats.work)?;
                    work.stats.node_visits += located.stats.node_visits;
                    let slice = located
                        .slice
                        .ok_or(PlanError::InvalidPlan("sound route sample has no interval"))?;
                    let start = grid.boundary(slice.destination.start)?.0;
                    let end = grid.boundary(slice.destination.end)?.0;
                    if start > cursor || end <= cursor {
                        return Err(PlanError::InvalidPlan(
                            "sound route interval does not own its sample",
                        ));
                    }
                    let piece = cursor..end.min(samples.end);
                    if let Some(recipe) = slice.recipe {
                        let old_start = self.grids[input].boundary(recipe.start)?.0;
                        let old_end = self.grids[input].boundary(recipe.end)?.0;
                        let delta = i128::from(old_start) - i128::from(start);
                        self.walk(
                            input,
                            shifted(piece.clone(), delta)?,
                            old_start..old_end,
                            destination_shift - delta,
                            work,
                        )?;
                    } else {
                        work.push(piece.clone(), destination_shift, None)?;
                    }
                    cursor = piece.end;
                }
                Ok(())
            }
        }
    }
}

impl QueryWork {
    fn spend(&mut self, amount: usize) -> Result<(), PlanError> {
        self.stats.work = self
            .stats
            .work
            .checked_add(amount)
            .filter(|total| *total <= self.limits.maximum_work)
            .ok_or(PlanError::AudioQueryLimit("sound route work"))?;
        Ok(())
    }

    fn push(
        &mut self,
        samples: Range<i64>,
        shift: i128,
        local_at_start: Option<ExactRatio>,
    ) -> Result<(), PlanError> {
        self.spend(1)?;
        let samples = shifted(samples, shift)?;
        if let Some(last) = self.spans.last_mut()
            && last.samples.end == samples.start
        {
            let continuous = match (last.local_at_start, local_at_start) {
                (None, None) => true,
                (Some(previous), Some(next)) => {
                    let count = i128::from(last.samples.end) - i128::from(last.samples.start);
                    previous.checked_add(ExactRatio::new(count, 1)?.checked_mul(self.step)?)?
                        == next
                }
                _ => false,
            };
            if continuous {
                last.samples.end = samples.end;
                return Ok(());
            }
        }
        if self.spans.len() == self.limits.maximum_spans {
            return Err(PlanError::AudioQueryLimit("sound route spans"));
        }
        self.spans.push(RawSpan {
            samples,
            local_at_start,
        });
        Ok(())
    }
}

fn shifted(samples: Range<i64>, shift: i128) -> Result<Range<i64>, TimeError> {
    let at = |value: i64| i64::try_from(i128::from(value) + shift).map_err(|_| TimeError::Overflow);
    Ok(at(samples.start)?..at(samples.end)?)
}

// Do not expose a generic numeric conversion trait for branded sample clocks.
// Root and intrinsic callers receive maps in their own consuming sample domain.
macro_rules! sample_domain {
    ($sample:ident, $rule:ident) => {
        impl AudioSoundRoute<$sample> {
            pub fn new(
                route: SoundRoute,
                grids: Vec<AudioSampleGrid<$sample>>,
            ) -> Result<Self, PlanError> {
                Self::new_inner(
                    route,
                    grids
                        .into_iter()
                        .map(|grid| {
                            AudioSampleGrid::new(
                                grid.frame_origin(),
                                grid.frames_per_sample(),
                                grid.boundary_rule(),
                            )
                        })
                        .collect::<Result<_, _>>()?,
                    AudioBoundaryRule::$rule,
                )
            }

            pub fn samples(&self) -> Range<$sample> {
                let allocation = &self.allocations[self.route.root() as usize];
                $sample(allocation.start)..$sample(allocation.end)
            }

            /// Original complete Recipe clock, before any Window or Ripple.
            /// Its integral labels identify retained provider samples; the
            /// current output grid cannot recover those labels after edits.
            pub fn recipe_grid(&self) -> AudioSampleGrid<$sample> {
                // SoundRoute validates an earlier-only unary history with
                // complete reachability, so its only Recipe is node zero.
                self.grids[0].rebrand()
            }

            /// Original complete Recipe allocation, independent of later
            /// selected audible masks and the current output allocation.
            pub fn recipe_samples(&self) -> Range<$sample> {
                let allocation = &self.allocations[0];
                $sample(allocation.start)..$sample(allocation.end)
            }

            pub fn query(
                &self,
                samples: Range<$sample>,
                limits: AudioQueryLimits,
            ) -> Result<AudioSoundRouteQuery<$sample>, PlanError> {
                let work = self.query_inner(samples.start.0..samples.end.0, limits)?;
                let spans = work
                    .spans
                    .into_iter()
                    .map(|span| {
                        Ok(AudioSoundRouteSpan {
                            sampling: span
                                .local_at_start
                                .map(|local| {
                                    AudioSampleMap::new(
                                        $sample(span.samples.start),
                                        local,
                                        work.step,
                                    )
                                })
                                .transpose()?,
                            samples: $sample(span.samples.start)..$sample(span.samples.end),
                        })
                    })
                    .collect::<Result<_, PlanError>>()?;
                Ok(AudioSoundRouteQuery {
                    samples,
                    spans,
                    stats: work.stats,
                })
            }
        }
    };
}

sample_domain!(AudioSample, RoundEven);
sample_domain!(SignalSample, PointCeil);
