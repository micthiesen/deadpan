//! Scalar support shared by full independent voices and planning-only discovery.

use super::*;

pub(super) struct Path {
    pub(super) host: usize,
    pub(super) edges: Vec<Edge>,
    pub(super) depth: usize,
    pub(super) has_processing: bool,
    pub(super) unclipped_before: usize,
    pub(super) support: Range<ExactRatio>,
}

pub(super) fn path(
    plan: &RenderPlan,
    instance: &InstancePath,
    context: Option<&AudioSoundOwnerContext<'_>>,
    budget: &mut BuildBudget,
) -> Result<Path, PlanError> {
    if instance.repeats.len() > MAX_DOCUMENT_DEPTH {
        return Err(PlanError::AudioQueryLimit("source occurrence depth"));
    }
    let host = *plan
        .by_id
        .get(&instance.node)
        .ok_or(PlanError::InvalidAudioSourceOccurrence(
            "source occurrence owner is absent",
        ))?;
    if plan.nodes[host].preserve_input.is_some() {
        return Err(PlanError::InvalidAudioSourceOccurrence(
            "owned Preserve occurrence requires its retained output placement",
        ));
    }
    let (edges, depth, has_processing) = resolve_edges(plan, host, instance, budget)?;
    let unclipped_before = match context {
        Some(context) => {
            budget.spend(1 + instance.repeats.len())?;
            for edge in &edges {
                budget.spend(2 * (1 + edge.repeats))?;
            }
            context.clip_boundary(plan, host, instance, &edges)?
        }
        None => 0,
    };
    let support = node_support(plan, host);
    if !positive_range(&support)? {
        return Err(PlanError::InvalidAudioSourceOccurrence(
            "source occurrence owner is empty",
        ));
    }
    Ok(Path {
        host,
        edges,
        depth,
        has_processing,
        unclipped_before,
        support,
    })
}

pub(super) fn processes(plan: &RenderPlan, edge: &Edge) -> bool {
    matches!(plan.nodes[edge.parent].kind,
        CompiledKind::Retime { pitch, scale, .. } if pitch.processes(scale == ExactRatio::ONE))
}

pub(super) struct Geometry {
    pub(super) support: Range<ExactRatio>,
    pub(super) extent: Range<ExactRatio>,
    pub(super) routing_origin: ExactRatio,
    pub(super) audible_extent: Range<ExactRatio>,
}

/// Processing receives the preceding scalar support before a Preserve resets
/// it to its complete output. Only the full constructor builds that processor;
/// discovery falls back to that constructor whenever any such edge is present.
pub(super) fn resolve(
    plan: &RenderPlan,
    path: &Path,
    mut audible: Range<ExactRatio>,
    budget: &mut BuildBudget,
    mut process: impl FnMut(
        &Edge,
        usize,
        Placement,
        Range<ExactRatio>,
        &mut BuildBudget,
    ) -> Result<(), PlanError>,
) -> Result<Geometry, PlanError> {
    let mut support = path.support.clone();
    let mut delivery_support = support.clone();
    let mut placement = Placement::IDENTITY;
    let mut stages = 0;
    for (index, edge) in path.edges.iter().enumerate().rev() {
        if processes(plan, edge) {
            stages += 1;
            process(edge, stages, placement, support, budget)?;
            placement = Placement::IDENTITY;
            support = node_support(plan, edge.parent);
            delivery_support = support.clone();
            audible = support.clone();
        } else {
            budget.spend(1)?;
            placement = edge.map.compose(placement)?;
            let allowed = node_support(plan, edge.parent);
            delivery_support = intersect(
                delivery_support,
                placement.local(allowed.start)?..placement.local(allowed.end)?,
            )?;
            if index >= path.unclipped_before {
                support = intersect(
                    support,
                    placement.local(allowed.start)?..placement.local(allowed.end)?,
                )?;
            }
            if !positive_range(&support)? {
                return Err(PlanError::InvalidAudioSourceOccurrence(
                    "source occurrence output is not visible",
                ));
            }
        }
    }
    Ok(Geometry {
        extent: placement.range(support.clone())?,
        routing_origin: placement.range(delivery_support)?.start,
        audible_extent: placement.range(intersect(audible, support.clone())?)?,
        support,
    })
}

pub(super) fn grid(plan: &RenderPlan) -> Result<AudioSampleGrid<AudioSample>, PlanError> {
    let rate = plan.metadata.presentation_basis.frame_rate;
    AudioSampleGrid::new(
        ExactRatio::ZERO,
        ExactRatio::new(
            i128::from(rate.numerator()),
            i128::from(MIX_SAMPLE_RATE) * i128::from(rate.denominator()),
        )?,
        AudioBoundaryRule::RoundEven,
    )
}

pub(in crate::plan) struct Discovered {
    pub(in crate::plan) samples: Range<AudioSample>,
    pub(in crate::plan) work: usize,
    pub(in crate::plan) runs: usize,
}

/// No provider graph escapes. Preserve candidates use the existing full
/// constructor, including all tape/projection guards. At fallback,
/// the resolved path is reused and its work stays in the full constructor's budget.
pub(in crate::plan) fn discover(
    plan: &RenderPlan,
    instance: &InstancePath,
    recipe: &AudioSourceVoiceRecipe,
    limits: AudioQueryLimits,
    context: Option<&AudioSoundOwnerContext<'_>>,
) -> Result<Discovered, PlanError> {
    let mut budget = BuildBudget {
        maximum: limits.maximum_work,
        remaining: limits.maximum_work,
        maximum_runs: limits.maximum_spans,
        runs: 0,
    };
    let path = path(plan, instance, context, &mut budget)?;
    if path.has_processing {
        let voice = plan.source_voice_occurrence_with_path(
            instance.clone(),
            recipe.clone(),
            context,
            path,
            budget,
        )?;
        return Ok(Discovered {
            samples: voice.samples(),
            work: voice.construction_work(),
            runs: voice.retained_runs(),
        });
    }
    // Validate the same scalar clocks used by gain owners and inherited edge
    // markers, without allocating their paths, vectors or provider identities.
    let step = grid(plan)?.frames_per_sample();
    let mut placement = Placement::IDENTITY;
    for (index, edge) in path.edges.iter().enumerate() {
        budget.spend(1)?;
        if plan.nodes[edge.parent].preserve_input.is_some() {
            return Err(PlanError::InvalidAudioSourceOccurrence(
                "owned Preserve occurrence requires its retained output placement",
            ));
        }
        if index >= path.unclipped_before {
            placement.range(node_support(plan, edge.parent))?;
            if let Some(gap) = &edge.gap {
                budget.spend(1)?;
                placement.range(gap.clone())?;
            }
        }
        owner_sampling(placement, step)?;
        placement = placement.compose(edge.map)?;
    }
    budget.spend(1)?;
    placement.range(path.support.clone())?;
    owner_sampling(placement, step)?;
    validate_owner_selection(plan, path.host, recipe)?;
    let selected = voice_selection(recipe, plan.metadata.presentation_basis.frame_rate)?;
    let geometry = resolve(
        plan,
        &path,
        selected.start..selected.end,
        &mut budget,
        |_, _, _, _, _| unreachable!("Preserve candidates use the validated fallback"),
    )?;
    budget.runs(1)?;
    validate_plain_tape(plan, path.host, path.support.clone(), &geometry, step)?;
    let grid = grid(plan)?;
    Ok(Discovered {
        samples: grid.boundary(geometry.extent.start)?..grid.boundary(geometry.extent.end)?,
        work: budget.maximum - budget.remaining,
        runs: budget.runs,
    })
}

// For a single ordinary source run, continuity and coverage follow by
// construction: its destination is the complete extent. Reuse the exact
// fallible remapping operations with a stack-only signal carrier. Transform
// evaluation is provider/path independent, so no provider Arc, Tape or gain
// owner is required. Preserve and policy-override runs never enter this path.
fn validate_plain_tape(
    plan: &RenderPlan,
    host: usize,
    host_support: Range<ExactRatio>,
    geometry: &Geometry,
    step: ExactRatio,
) -> Result<(), PlanError> {
    let extent = &geometry.extent;
    if !positive_range(extent)? {
        return Err(PlanError::InvalidPlan(
            "audio tape support must be positive",
        ));
    }
    let signal = scoped_signal(plan, host, Vec::new(), host_support);
    let mapped = signal.remap_for_tape(
        extent.clone(),
        extent.clone(),
        geometry.support.clone(),
        step,
        extent.start,
    )?;
    let point =
        AudioSampleGrid::<SignalSample>::new(extent.start, step, AudioBoundaryRule::PointCeil)?;
    point.boundary(extent.start)?;
    point.boundary(extent.end)?;
    // remap_policy_window maps extent onto itself. Its one-run rate is one;
    // still evaluate the checked length division used by that operation.
    let length = extent.end.checked_sub(extent.start)?;
    let rate = length.checked_div(length)?;
    let map = |source: ExactRatio| {
        extent
            .start
            .checked_add(source.checked_sub(extent.start)?.checked_div(rate)?)
    };
    let semantic_support = map(extent.start)?..map(extent.end)?;
    if semantic_support != *extent {
        return Err(PlanError::InvalidPlan(
            "audio tape remap did not cover destination",
        ));
    }
    mapped.remap_for_tape(
        semantic_support.clone(),
        semantic_support,
        extent.clone(),
        step,
        ExactRatio::ZERO,
    )?;
    Ok(())
}
