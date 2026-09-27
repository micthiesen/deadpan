//! Exact borrowed projections of current audio signal contexts.

use std::ops::Range;
use std::sync::Arc;

use deadpan_core::{ExactRatio, MIX_SAMPLE_RATE};

use crate::{
    AudioBoundaryRule, AudioPolicyQuery, AudioQueryLimits, AudioSampleGrid, AudioSignal,
    AudioSignalQuery, LookupStats, PlanError, RenderPlan, SignalSample,
};

const MAX_MATERIALIZED_TAPE_RUNS: usize = 65_536;

#[derive(Debug)]
struct TapeBuildBudget {
    remaining_runs: usize,
}

impl Default for TapeBuildBudget {
    fn default() -> Self {
        Self {
            remaining_runs: MAX_MATERIALIZED_TAPE_RUNS,
        }
    }
}

impl TapeBuildBudget {
    fn charge(&mut self, runs: usize) -> Result<(), PlanError> {
        self.remaining_runs =
            self.remaining_runs
                .checked_sub(runs)
                .ok_or(PlanError::AudioQueryLimit(
                    "audio tape projection expansion",
                ))?;
        Ok(())
    }
}

/// One exact destination run backed by a live signal window in the same plan.
/// Its increasing affine map sends destination coordinates to source signal
/// coordinates. The run window controls allocation only; signal filter support
/// remains the provider's own meaningful support.
#[derive(Debug, Clone)]
pub struct AudioSignalTapeRun<'plan> {
    destination: Range<ExactRatio>,
    source: Range<ExactRatio>,
    provider: TapeProvider<'plan>,
}

#[derive(Debug, Clone)]
enum TapeProvider<'plan> {
    Signal(Box<AudioSignal<'plan>>),
    Intrinsic(Arc<crate::AudioStageProjection<'plan>>),
}

impl<'plan> AudioSignalTapeRun<'plan> {
    pub fn new(
        destination: Range<ExactRatio>,
        source: Range<ExactRatio>,
        signal: AudioSignal<'plan>,
    ) -> Self {
        Self {
            destination,
            source,
            provider: TapeProvider::Signal(Box::new(signal)),
        }
    }

    /// Route a live projected Preserve stage's intrinsic output into this run.
    /// `source` is measured in the projection's normalized output clock.
    pub fn intrinsic(
        destination: Range<ExactRatio>,
        source: Range<ExactRatio>,
        projection: Arc<crate::AudioStageProjection<'plan>>,
    ) -> Self {
        Self {
            destination,
            source,
            provider: TapeProvider::Intrinsic(projection),
        }
    }

    pub fn destination(&self) -> Range<ExactRatio> {
        self.destination.clone()
    }

    pub fn source(&self) -> Range<ExactRatio> {
        self.source.clone()
    }
}

#[derive(Debug, Clone)]
struct PlacedRun<'plan> {
    destination: Range<ExactRatio>,
    samples: Range<SignalSample>,
    signal: AudioSignal<'plan>,
    projection: Option<Arc<crate::AudioStageProjection<'plan>>>,
    policy_override: Option<Box<AudioSignalTape<'plan>>>,
}

/// A bounded exact projection of current signal windows onto one PointCeil grid.
/// Runs keep physical query allocations separate from full source-filter support.
#[derive(Debug, Clone)]
pub struct AudioSignalTape<'plan> {
    plan: &'plan RenderPlan,
    /// Full meaningful signal support, including filter and Bound context.
    support: Range<ExactRatio>,
    /// Exact interval covered by these runs. A remapped policy view can retain
    /// wider semantic support while routing only one nested allocation window.
    route: Range<ExactRatio>,
    grid: AudioSampleGrid<SignalSample>,
    runs: Vec<PlacedRun<'plan>>,
}

impl<'plan> AudioSignalTape<'plan> {
    /// Build a complete exact partition of `full_support` from current signals.
    /// All providers are remapped to the same intrinsic PointCeil grid.
    pub fn new(
        plan: &'plan RenderPlan,
        full_support: Range<ExactRatio>,
        runs: Vec<AudioSignalTapeRun<'plan>>,
    ) -> Result<Self, PlanError> {
        let grid_origin = full_support.start;
        Self::build(plan, full_support, runs, grid_origin)
    }

    fn build(
        plan: &'plan RenderPlan,
        full_support: Range<ExactRatio>,
        runs: Vec<AudioSignalTapeRun<'plan>>,
        grid_origin: ExactRatio,
    ) -> Result<Self, PlanError> {
        if !positive_range(&full_support)? {
            return Err(PlanError::InvalidPlan(
                "audio tape support must be positive",
            ));
        }
        if runs.is_empty() || runs.len() > 4096 {
            return Err(PlanError::AudioQueryLimit("audio tape run count"));
        }
        let mut build_budget = TapeBuildBudget::default();
        build_budget.charge(runs.len())?;
        let rate = plan.metadata().presentation_basis.frame_rate;
        let frames_per_sample = ExactRatio::new(
            i128::from(rate.numerator()),
            i128::from(MIX_SAMPLE_RATE) * i128::from(rate.denominator()),
        )?;
        let grid: AudioSampleGrid<SignalSample> =
            AudioSampleGrid::new(grid_origin, frames_per_sample, AudioBoundaryRule::PointCeil)?;
        let mut expected_start = full_support.start;
        let mut placed = Vec::with_capacity(runs.len());
        for run in runs {
            if run.destination.start != expected_start || !positive_range(&run.destination)? {
                return Err(PlanError::InvalidPlan(
                    "audio tape runs must be positive and exactly contiguous",
                ));
            }
            if run
                .destination
                .start
                .checked_sub(full_support.start)?
                .compare_integer(0)
                .is_lt()
                || run
                    .destination
                    .end
                    .checked_sub(full_support.end)?
                    .compare_integer(0)
                    .is_gt()
            {
                return Err(PlanError::InvalidPlan(
                    "audio tape run is outside full support",
                ));
            }
            let (signal, projection, policy_override) = match run.provider {
                TapeProvider::Signal(signal) => {
                    if !signal.belongs_to(plan) {
                        return Err(PlanError::InvalidPlan(
                            "audio tape provider belongs to another plan",
                        ));
                    }
                    (
                        signal.remap_for_tape(
                            full_support.clone(),
                            run.destination.clone(),
                            run.source.clone(),
                            frames_per_sample,
                            grid_origin,
                        )?,
                        None,
                        None,
                    )
                }
                TapeProvider::Intrinsic(projection) => {
                    if !projection.belongs_to(plan) {
                        return Err(PlanError::InvalidPlan(
                            "projected Preserve belongs to another plan",
                        ));
                    }
                    let projection_support = projection.output_policy().support();
                    if run
                        .source
                        .start
                        .checked_sub(projection_support.start)?
                        .compare_integer(0)
                        .is_lt()
                        || run
                            .source
                            .end
                            .checked_sub(projection_support.end)?
                            .compare_integer(0)
                            .is_gt()
                    {
                        return Err(PlanError::InvalidPlan(
                            "projected Preserve source is outside its intrinsic output",
                        ));
                    }
                    let signal = AudioSignal::for_projection(plan, Arc::clone(&projection))?
                        .remap_for_tape(
                            full_support.clone(),
                            run.destination.clone(),
                            run.source.clone(),
                            frames_per_sample,
                            grid_origin,
                        )?;
                    let policy_override = Some(Box::new(projection.output_policy().remap_window(
                        run.source.clone(),
                        run.destination.clone(),
                        grid_origin,
                        frames_per_sample,
                        AudioBoundaryRule::PointCeil,
                        &mut build_budget,
                    )?));
                    (signal, Some(projection), policy_override)
                }
            };
            placed.push(PlacedRun {
                destination: run.destination.clone(),
                samples: grid.boundary(run.destination.start)?
                    ..grid.boundary(run.destination.end)?,
                signal,
                projection,
                policy_override,
            });
            expected_start = run.destination.end;
        }
        if expected_start != full_support.end {
            return Err(PlanError::InvalidPlan(
                "audio tape runs do not cover full support",
            ));
        }
        Ok(Self {
            plan,
            support: full_support.clone(),
            route: full_support,
            grid,
            runs: placed,
        })
    }

    pub fn belongs_to(&self, plan: &RenderPlan) -> bool {
        std::ptr::eq(self.plan, plan)
    }

    pub fn support(&self) -> Range<ExactRatio> {
        self.support.clone()
    }

    pub fn grid(&self) -> AudioSampleGrid<SignalSample> {
        self.grid
    }

    pub(crate) fn covers_support(&self) -> bool {
        self.route == self.support
    }

    fn sample_start(&self) -> Result<SignalSample, PlanError> {
        Ok(self.grid.boundary(self.route.start)?)
    }

    /// Number of samples on the common PointCeil grid, independent of run splits.
    pub fn sample_count(&self) -> Result<SignalSample, PlanError> {
        Ok(self.grid.boundary(self.support.end)?)
    }

    /// Query current signal content while charging all run dispatch and traversal
    /// to one caller-supplied work/span allowance.
    pub fn query(
        &self,
        samples: Range<SignalSample>,
        limits: AudioQueryLimits,
    ) -> Result<AudioSignalQuery<'plan>, PlanError> {
        limits.validate()?;
        let count = self.sample_count()?;
        let route_end = self.grid.boundary(self.route.end)?;
        if samples.start < self.sample_start()?
            || samples.end < samples.start
            || samples.end > route_end
            || samples.end > count
        {
            return Err(PlanError::AudioRangeOutOfRange);
        }
        let mut spans = Vec::new();
        let mut lookup = LookupStats::default();
        let mut work = 0_usize;
        let mut cursor = samples.start;
        while cursor < samples.end {
            let index = self.run_for(cursor, &mut work, limits.maximum_work)?;
            let run = self
                .runs
                .get(index)
                .ok_or(PlanError::InvalidPlan("audio tape sample is uncovered"))?;
            let end = samples.end.min(run.samples.end);
            if cursor < run.samples.start || end <= cursor {
                return Err(PlanError::InvalidPlan(
                    "audio tape dispatch did not advance",
                ));
            }
            charge(&mut work, 1, limits.maximum_work)?;
            let remaining_spans = limits
                .maximum_spans
                .checked_sub(spans.len())
                .ok_or(PlanError::AudioQueryLimit("audio tape span count"))?;
            let remaining_work = limits
                .maximum_work
                .checked_sub(work)
                .ok_or(PlanError::AudioQueryLimit("audio tape work"))?;
            if remaining_spans == 0 || remaining_work == 0 {
                return Err(PlanError::AudioQueryLimit("audio tape work/span"));
            }
            let query = run.signal.query(
                cursor..end,
                AudioQueryLimits {
                    maximum_spans: remaining_spans,
                    maximum_work: remaining_work,
                },
            )?;
            charge(&mut work, query.work, limits.maximum_work)?;
            add_lookup(&mut lookup, query.lookup)?;
            spans.extend(query.spans);
            if spans.len() > limits.maximum_spans {
                return Err(PlanError::AudioQueryLimit("audio tape span count"));
            }
            cursor = end;
        }
        Ok(AudioSignalQuery {
            project_id: self.plan.metadata().project_id.clone(),
            revision_id: self.plan.metadata().revision_id.clone(),
            // A tape can cross authored definitions; each returned span keeps
            // its own definition scope, so there is no single query alias.
            definition: None,
            samples: samples.clone(),
            spans,
            lookup,
            work,
        })
    }

    /// Resolve policy over the same exact runs and charge one shared allowance.
    pub fn policy(
        &self,
        samples: Range<SignalSample>,
        limits: AudioQueryLimits,
    ) -> Result<AudioPolicyQuery<SignalSample>, PlanError> {
        self.policy_inner(samples, limits, false)
    }

    /// Resolve explicit silence after a Preserve stage. Physical Source
    /// endpoint masks belong to its input; silent Holds remain output policy.
    pub fn policy_after_preserve(
        &self,
        samples: Range<SignalSample>,
        limits: AudioQueryLimits,
    ) -> Result<AudioPolicyQuery<SignalSample>, PlanError> {
        self.policy_inner(samples, limits, true)
    }

    fn policy_inner(
        &self,
        samples: Range<SignalSample>,
        limits: AudioQueryLimits,
        after_preserve: bool,
    ) -> Result<AudioPolicyQuery<SignalSample>, PlanError> {
        limits.validate()?;
        let count = self.sample_count()?;
        let route_end = self.grid.boundary(self.route.end)?;
        if samples.start < self.sample_start()?
            || samples.end < samples.start
            || samples.end > route_end
            || samples.end > count
        {
            return Err(PlanError::AudioRangeOutOfRange);
        }
        let mut suppressed = Vec::new();
        let mut contents = Vec::new();
        let mut lookup = LookupStats::default();
        let mut work = 0_usize;
        let mut cursor = samples.start;
        while cursor < samples.end {
            let index = self.run_for(cursor, &mut work, limits.maximum_work)?;
            let run = self.runs.get(index).ok_or(PlanError::InvalidPlan(
                "audio tape policy sample is uncovered",
            ))?;
            let end = samples.end.min(run.samples.end);
            if cursor < run.samples.start || end <= cursor {
                return Err(PlanError::InvalidPlan("audio tape policy did not advance"));
            }
            charge(&mut work, 1, limits.maximum_work)?;
            let remaining_work = limits
                .maximum_work
                .checked_sub(work)
                .ok_or(PlanError::AudioQueryLimit("audio tape policy work"))?;
            if remaining_work == 0 {
                return Err(PlanError::AudioQueryLimit("audio tape policy work"));
            }
            let query_limits = AudioQueryLimits {
                // Policy span limits apply to each logical provider query;
                // recursive content and masks share the aggregate work cap.
                maximum_spans: limits.maximum_spans,
                maximum_work: remaining_work,
            };
            let policy = if let Some(view) = &run.policy_override {
                view.policy_after_preserve(cursor..end, query_limits)?
            } else {
                run.signal.policy_on_grid(
                    cursor..end,
                    query_limits,
                    self.grid.boundary_rule(),
                    !after_preserve,
                )?
            };
            charge(&mut work, policy.work, limits.maximum_work)?;
            add_lookup(&mut lookup, policy.lookup)?;
            suppressed.extend(policy.suppressed);
            contents.extend(policy.contents);
            cursor = end;
        }
        suppressed.sort_unstable_by_key(|range| range.start);
        let mut merged: Vec<Range<SignalSample>> = Vec::new();
        for range in suppressed {
            if range.start >= range.end {
                continue;
            }
            if let Some(last) = merged.last_mut()
                && range.start <= last.end
            {
                last.end = last.end.max(range.end);
            } else {
                merged.push(range);
            }
        }
        Ok(AudioPolicyQuery {
            suppressed: merged,
            contents,
            lookup,
            work,
        })
    }

    fn remap_window(
        &self,
        source_window: Range<ExactRatio>,
        destination: Range<ExactRatio>,
        grid_origin: ExactRatio,
        frames_per_sample: ExactRatio,
        boundary_rule: AudioBoundaryRule,
        build_budget: &mut TapeBuildBudget,
    ) -> Result<Self, PlanError> {
        if !positive_range(&source_window)? || !positive_range(&destination)? {
            return Err(PlanError::InvalidPlan(
                "audio tape remap ranges must be positive",
            ));
        }
        if source_window
            .start
            .checked_sub(self.route.start)?
            .compare_integer(0)
            .is_lt()
            || source_window
                .end
                .checked_sub(self.route.end)?
                .compare_integer(0)
                .is_gt()
        {
            return Err(PlanError::InvalidPlan(
                "audio tape remap source is outside support",
            ));
        }
        let rate = source_window
            .end
            .checked_sub(source_window.start)?
            .checked_div(destination.end.checked_sub(destination.start)?)?;
        let map_to_destination = |source_frame: ExactRatio| {
            destination.start.checked_add(
                source_frame
                    .checked_sub(source_window.start)?
                    .checked_div(rate)?,
            )
        };
        let semantic_support =
            map_to_destination(self.support.start)?..map_to_destination(self.support.end)?;
        let grid: AudioSampleGrid<SignalSample> =
            AudioSampleGrid::new(grid_origin, frames_per_sample, boundary_rule)?;
        build_budget.charge(self.runs.len())?;
        let mut placed = Vec::new();
        let mut expected_start = destination.start;
        for run in &self.runs {
            let start = if run
                .destination
                .start
                .checked_sub(source_window.start)?
                .compare_integer(0)
                .is_gt()
            {
                run.destination.start
            } else {
                source_window.start
            };
            let end = if run
                .destination
                .end
                .checked_sub(source_window.end)?
                .compare_integer(0)
                .is_lt()
            {
                run.destination.end
            } else {
                source_window.end
            };
            if end.checked_sub(start)?.compare_integer(0).is_le() {
                continue;
            }
            let destination_run = map_to_destination(start)?..map_to_destination(end)?;
            if destination_run.start != expected_start {
                return Err(PlanError::InvalidPlan(
                    "audio tape remap did not preserve contiguous runs",
                ));
            }
            let signal = run.signal.remap_for_tape(
                semantic_support.clone(),
                destination_run.clone(),
                start..end,
                frames_per_sample,
                grid_origin,
            )?;
            let policy_override = match &run.policy_override {
                Some(view) => Some(Box::new(view.remap_window(
                    start..end,
                    destination_run.clone(),
                    grid_origin,
                    frames_per_sample,
                    boundary_rule,
                    build_budget,
                )?)),
                None => None,
            };
            placed.push(PlacedRun {
                destination: destination_run.clone(),
                samples: grid.boundary(destination_run.start)?
                    ..grid.boundary(destination_run.end)?,
                signal,
                projection: run.projection.clone(),
                policy_override,
            });
            expected_start = destination_run.end;
        }
        if expected_start != destination.end {
            return Err(PlanError::InvalidPlan(
                "audio tape remap did not cover destination",
            ));
        }
        Ok(Self {
            plan: self.plan,
            support: semantic_support,
            route: destination,
            grid,
            runs: placed,
        })
    }

    pub(crate) fn remap_policy_window(
        &self,
        source_window: Range<ExactRatio>,
        destination: Range<ExactRatio>,
        grid_origin: ExactRatio,
        frames_per_sample: ExactRatio,
        boundary_rule: AudioBoundaryRule,
    ) -> Result<Self, PlanError> {
        let mut build_budget = TapeBuildBudget::default();
        self.remap_window(
            source_window,
            destination,
            grid_origin,
            frames_per_sample,
            boundary_rule,
            &mut build_budget,
        )
    }

    pub(crate) fn validate_projection_scope(
        &self,
        owner: &crate::AudioStage<'_>,
        budget: &mut crate::audio_projection::ProjectionValidationBudget,
        depth: usize,
    ) -> Result<usize, PlanError> {
        let mut relative_depth = 0;
        for run in &self.runs {
            budget.edge()?;
            match &run.projection {
                Some(projection) => {
                    if !owner.accepts_projection_stage(projection.stage()) {
                        return Err(PlanError::InvalidPlan(
                            "projected Preserve provider is outside its owner scope",
                        ));
                    }
                    relative_depth = relative_depth.max(projection.validate_graph(budget, depth)?);
                }
                None => {
                    if !owner.accepts_projection_signal(&run.signal) {
                        return Err(PlanError::InvalidPlan(
                            "audio signal provider is outside its Preserve owner scope",
                        ));
                    }
                }
            }
        }
        Ok(relative_depth)
    }

    fn run_for(
        &self,
        sample: SignalSample,
        work: &mut usize,
        maximum: usize,
    ) -> Result<usize, PlanError> {
        let mut left = 0;
        let mut right = self.runs.len();
        while left < right {
            charge(work, 1, maximum)?;
            let middle = left + (right - left) / 2;
            if self.runs[middle].samples.end <= sample {
                left = middle + 1;
            } else {
                right = middle;
            }
        }
        Ok(left)
    }
}

fn positive_range(range: &Range<ExactRatio>) -> Result<bool, PlanError> {
    Ok(range
        .end
        .checked_sub(range.start)?
        .compare_integer(0)
        .is_gt())
}

fn charge(used: &mut usize, amount: usize, maximum: usize) -> Result<(), PlanError> {
    *used = used
        .checked_add(amount)
        .ok_or(PlanError::AudioQueryLimit("audio tape work"))?;
    if *used > maximum {
        return Err(PlanError::AudioQueryLimit("audio tape work"));
    }
    Ok(())
}

fn add_lookup(total: &mut LookupStats, next: LookupStats) -> Result<(), PlanError> {
    total.visited_nodes = total
        .visited_nodes
        .checked_add(next.visited_nodes)
        .ok_or(PlanError::AudioQueryLimit("audio tape lookup count"))?;
    total.sequence_comparisons = total
        .sequence_comparisons
        .checked_add(next.sequence_comparisons)
        .ok_or(PlanError::AudioQueryLimit("audio tape lookup count"))?;
    total.iteration_run_comparisons = total
        .iteration_run_comparisons
        .checked_add(next.iteration_run_comparisons)
        .ok_or(PlanError::AudioQueryLimit("audio tape lookup count"))?;
    Ok(())
}
