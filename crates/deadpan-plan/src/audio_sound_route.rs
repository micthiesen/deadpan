//! Retained physical sample clocks for exact sound routes. A frame-only flattening
//! cannot reproduce successive rounded edit seams. Each history entry therefore
//! retains its own allocation grid, and every Keep resumes the preceding entry
//! at that entry's physical sample boundary. Recipe phase is never reconstructed
//! from the final picture coordinate or a rounded duration ratio.

use std::{marker::PhantomData, ops::Range};

use deadpan_core::{
    AudioSample, ExactRatio, MAX_SOUND_ROUTE_EDGES, SoundRoute, SoundRouteNode,
    SoundRouteQueryLimits, SoundRouteQueryStats, TimeError,
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
    identity_anchor: ExactRatio,
    // Explicit sample translations for a proved identity placement chain.
    // The anchor may change when a new crop becomes the delivery boundary.
    occurrence: Option<OccurrenceChain>,
    domain: PhantomData<S>,
}

/// One chronological delivery window in a complete occurrence's unchanged
/// recipe coordinates. `selection` clips delivered samples, never source or DSP
/// history. `anchor` names the boundary used to copy old integral sample labels
/// into this placement, independently of the selected processing allocation.
/// The first entry must select the full recipe; later entries cannot revive
/// samples discarded by an earlier window, even when their selection grows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioOccurrenceWindow {
    pub selection: Range<ExactRatio>,
    pub anchor: ExactRatio,
}

#[derive(Debug, Clone)]
struct OccurrenceChain {
    windows: Vec<AudioOccurrenceWindow>,
    deltas: Vec<i128>,
}

pub(crate) struct OccurrenceTransition {
    pub allocation: Range<AudioSample>,
    pub frame_shift: ExactRatio,
    pub sample_shift: i128,
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

enum QueryTask {
    Walk {
        id: usize,
        samples: Range<i64>,
        allocation: Range<i64>,
        shift: i128,
    },
    Silence {
        samples: Range<i64>,
        shift: i128,
    },
    Ripple {
        id: usize,
        samples: Range<i64>,
        shift: i128,
    },
}

impl<S> AudioSoundRoute<S> {
    pub fn route(&self) -> &SoundRoute {
        &self.route
    }

    pub(crate) fn identity_anchor(&self) -> ExactRatio {
        self.identity_anchor
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
            identity_anchor: ExactRatio::ZERO,
            occurrence: None,
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

    // Retain chronological sample clocks without consuming the call stack.
    // Ripple continuations locate only the next reached piece, so a compact
    // Repeat remains bounded by query work rather than its expanded play count.
    fn walk(
        &self,
        id: usize,
        samples: Range<i64>,
        allocation: Range<i64>,
        destination_shift: i128,
        work: &mut QueryWork,
    ) -> Result<(), PlanError> {
        let mut tasks = vec![QueryTask::Walk {
            id,
            samples,
            allocation,
            shift: destination_shift,
        }];
        while let Some(task) = tasks.pop() {
            match task {
                QueryTask::Silence { samples, shift } => work.push(samples, shift, None)?,
                QueryTask::Walk {
                    id,
                    samples,
                    allocation,
                    shift,
                } => {
                    if samples.is_empty() {
                        continue;
                    }
                    work.spend(1)?;
                    work.stats.node_visits += 1;
                    let first = allocation.start.max(self.allocations[id].start);
                    let allocation = first..allocation.end.min(self.allocations[id].end).max(first);
                    // Allocation masks do not narrow the complete recipe/DSP
                    // support. Never expose a rounded sample past an old cut.
                    if samples.start < allocation.start {
                        work.push(
                            samples.start..samples.end.min(allocation.start),
                            shift,
                            None,
                        )?;
                    }
                    if samples.end > allocation.end {
                        tasks.push(QueryTask::Silence {
                            samples: samples.start.max(allocation.end)..samples.end,
                            shift,
                        });
                    }
                    let samples =
                        samples.start.max(allocation.start)..samples.end.min(allocation.end);
                    if samples.is_empty() {
                        continue;
                    }
                    match &self.route.nodes()[id] {
                        SoundRouteNode::Recipe {} => work.push(
                            samples.clone(),
                            shift,
                            Some(self.grids[id].at(ReferenceSample(samples.start))?),
                        )?,
                        SoundRouteNode::Window { input, selection } => {
                            let input = *input as usize;
                            let old_start = self.grids[input].boundary(selection.start)?.0;
                            let old_end = self.grids[input].boundary(selection.end)?.0;
                            let delta =
                                i128::from(old_start) - i128::from(self.allocations[id].start);
                            tasks.push(QueryTask::Walk {
                                id: input,
                                samples: shifted(samples, delta)?,
                                allocation: old_start..old_end,
                                shift: shift - delta,
                            });
                        }
                        SoundRouteNode::Ripple { .. } => {
                            tasks.push(QueryTask::Ripple { id, samples, shift })
                        }
                    }
                }
                QueryTask::Ripple { id, samples, shift } => {
                    let SoundRouteNode::Ripple { input, map } = &self.route.nodes()[id] else {
                        return Err(PlanError::InvalidPlan(
                            "sound route continuation is not a Ripple",
                        ));
                    };
                    let input = *input as usize;
                    let grid = &self.grids[id];
                    let cursor = samples.start;
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
                    if piece.end < samples.end {
                        tasks.push(QueryTask::Ripple {
                            id,
                            samples: piece.end..samples.end,
                            shift,
                        });
                    }
                    if let Some(recipe) = slice.recipe {
                        let old_start = self.grids[input].boundary(recipe.start)?.0;
                        let old_end = self.grids[input].boundary(recipe.end)?.0;
                        let delta = self.occurrence.as_ref().map_or_else(
                            || i128::from(old_start) - i128::from(start),
                            |chain| chain.deltas[id],
                        );
                        tasks.push(QueryTask::Walk {
                            id: input,
                            samples: shifted(piece, delta)?,
                            allocation: old_start..old_end,
                            shift: shift - delta,
                        });
                    } else {
                        work.push(piece, shift, None)?;
                    }
                }
            }
            if tasks.len() > MAX_SOUND_ROUTE_EDGES {
                return Err(PlanError::AudioQueryLimit("sound route pending intervals"));
            }
        }
        Ok(())
    }
}

impl AudioSoundRoute<AudioSample> {
    /// Keep the rounded placement anchor when an occurrence's complete input
    /// extends behind an exterior crop. Every route operation must be a full
    /// identity Keep; this cannot change the seams of a Window or edited map.
    /// The checked occurrence constructor verifies this against its own anchor.
    pub fn with_identity_anchor(self, anchor: ExactRatio) -> Result<Self, PlanError> {
        let windows = vec![
            AudioOccurrenceWindow {
                selection: ExactRatio::ZERO..self.route.recipe_extent(),
                anchor,
            };
            self.grids.len()
        ];
        self.with_occurrence_windows(windows)
    }

    /// Retain each move/crop's physical selection and rounding boundary while
    /// keeping the complete occurrence recipe available for preparation. The
    /// route must be a linear chain of full identity Keeps; its grids place that
    /// same virtual allocation at every step. Windows select only delivery.
    /// The first anchor remains checked by `AudioRoutedRoot::occurrence`.
    pub fn with_occurrence_windows(
        mut self,
        windows: Vec<AudioOccurrenceWindow>,
    ) -> Result<Self, PlanError> {
        let extent = self.route.recipe_extent();
        if windows.len() != self.grids.len()
            || windows
                .first()
                .is_none_or(|window| window.selection != (ExactRatio::ZERO..extent))
            || self.route.root() as usize != self.route.nodes().len() - 1
        {
            return Err(PlanError::InvalidPlan(
                "sound occurrence windows require a complete identity chain",
            ));
        }
        for (index, node) in self.route.nodes().iter().enumerate() {
            let valid = match node {
                SoundRouteNode::Recipe {} => index == 0,
                SoundRouteNode::Ripple { input, map } => {
                    index > 0
                        && *input as usize == index - 1
                        && map.input_extent() == extent
                        && map.output_extent() == extent
                        && map.root() == 0
                        && matches!(map.nodes(), [deadpan_core::SoundRippleNode::Keep { range }]
                            if range.start == ExactRatio::ZERO && range.end == extent)
                }
                SoundRouteNode::Window { .. } => false,
            };
            if !valid {
                return Err(PlanError::InvalidPlan(
                    "sound occurrence windows require a complete identity chain",
                ));
            }
        }
        let mut deltas = Vec::with_capacity(windows.len());
        for (index, window) in windows.iter().enumerate() {
            if window.selection.start.compare(ExactRatio::ZERO).is_lt()
                || !window.selection.end.compare(window.selection.start).is_gt()
                || window.selection.end.compare(extent).is_gt()
            {
                return Err(PlanError::InvalidPlan(
                    "sound occurrence window exceeds its complete recipe",
                ));
            }
            let grid = &self.grids[index];
            self.allocations[index] =
                grid.boundary(window.selection.start)?.0..grid.boundary(window.selection.end)?.0;
            let current = grid.boundary(window.anchor)?.0;
            deltas.push(if index == 0 {
                0
            } else {
                i128::from(self.grids[index - 1].boundary(window.anchor)?.0) - i128::from(current)
            });
        }
        self.identity_anchor = windows[0].anchor;
        self.occurrence = Some(OccurrenceChain { windows, deltas });
        Ok(self)
    }

    pub(crate) fn occurrence_output(&self) -> Result<(Range<ExactRatio>, ExactRatio), PlanError> {
        let chain = self.occurrence.as_ref().ok_or(PlanError::InvalidPlan(
            "sound envelope requires occurrence windows",
        ))?;
        let index = self.grids.len() - 1;
        let origin = self.grids[index].frame_origin();
        let window = &chain.windows[index];
        Ok((
            window.selection.start.checked_sub(origin)?
                ..window.selection.end.checked_sub(origin)?,
            window.anchor.checked_sub(origin)?,
        ))
    }

    /// Input of a retained temporal owner, before any exterior delivery mask.
    /// Every expanded selection must still be the same complete operand. Its
    /// physical labels translate by the proved chronological delivery anchors;
    /// newly rounded output allocations cannot truncate filter history.
    pub(crate) fn complete_processing_history(&self) -> Result<Option<Self>, PlanError> {
        let Some(chain) = &self.occurrence else {
            return Err(PlanError::InvalidPlan(
                "sound history requires occurrence windows",
            ));
        };
        let extent = self.route.recipe_extent();
        if chain
            .windows
            .iter()
            .any(|window| window.selection != (ExactRatio::ZERO..extent))
        {
            return Ok(None);
        }
        let shift = chain.deltas.iter().try_fold(0_i128, |shift, delta| {
            shift.checked_sub(*delta).ok_or(TimeError::Overflow)
        })?;
        let first = self.recipe_grid();
        let last = first.translated_samples(shift)?;
        let keep = deadpan_core::SoundRippleMap::new(
            extent,
            0,
            vec![deadpan_core::SoundRippleNode::Keep {
                range: deadpan_core::ExactFrameRange::new(ExactRatio::ZERO, extent)?,
            }],
        )?;
        let route = SoundRoute::identity(extent)?.ripple(keep)?;
        Ok(Some(
            Self::new(route, vec![first, last])?.with_identity_anchor(self.identity_anchor)?,
        ))
    }

    pub(crate) fn occurrence_transitions(
        &self,
    ) -> Result<
        impl ExactSizeIterator<Item = Result<OccurrenceTransition, PlanError>> + '_,
        PlanError,
    > {
        let chain = self.occurrence.as_ref().ok_or(PlanError::InvalidPlan(
            "sound envelope requires occurrence windows",
        ))?;
        Ok((1..self.grids.len()).map(|index| {
            Ok(OccurrenceTransition {
                allocation: AudioSample(self.allocations[index].start)
                    ..AudioSample(self.allocations[index].end),
                frame_shift: self.grids[index - 1]
                    .frame_origin()
                    .checked_sub(self.grids[index].frame_origin())?,
                sample_shift: -chain.deltas[index],
            })
        }))
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
                    grids.into_iter().map(|grid| grid.rebrand()).collect(),
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

#[cfg(test)]
mod processing_history_tests {
    use super::*;

    #[test]
    fn complete_history_keeps_half_sample_endpoints_after_an_odd_translation() {
        let extent = ExactRatio::integer(3);
        let keep = deadpan_core::SoundRippleMap::new(
            extent,
            0,
            vec![deadpan_core::SoundRippleNode::Keep {
                range: deadpan_core::ExactFrameRange::new(ExactRatio::ZERO, extent).unwrap(),
            }],
        )
        .unwrap();
        let route = SoundRoute::identity(extent).unwrap().ripple(keep).unwrap();
        let grid = |origin| {
            AudioSampleGrid::new(origin, ExactRatio::ONE, AudioBoundaryRule::RoundEven).unwrap()
        };
        let delivery = AudioSoundRoute::<AudioSample>::new(
            route,
            vec![
                grid(ExactRatio::new(1, 2).unwrap()),
                grid(ExactRatio::integer(-1)),
            ],
        )
        .unwrap()
        .with_identity_anchor(ExactRatio::ZERO)
        .unwrap();
        let input = delivery.complete_processing_history().unwrap().unwrap();
        assert_eq!(delivery.recipe_samples(), AudioSample(0)..AudioSample(2));
        assert_eq!(delivery.samples(), AudioSample(1)..AudioSample(4));
        assert_eq!(input.samples(), AudioSample(1)..AudioSample(3));
        let query = input.query(input.samples(), Default::default()).unwrap();
        assert_eq!(query.spans.len(), 1);
        let mapping = query.spans[0].sampling.unwrap();
        for sample in 1..3 {
            assert_eq!(
                mapping.local_at(AudioSample(sample)).unwrap(),
                delivery.recipe_grid().at(AudioSample(sample - 1)).unwrap()
            );
        }
    }
}
