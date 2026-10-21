//! Rate classes for one live Repeat axis. Origins and support belong only to
//! each returned representative; they do not prove emptiness for its class.

use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RepeatOwnerUse {
    DefaultBody,
    OwnedBody {
        iteration: IterationId,
        root: NodeId,
    },
    DefaultGap,
    OwnedGap {
        iteration: IterationId,
        root: NodeId,
    },
}

/// Both references must describe the same pre-edit document. The caller builds
/// this indexed snapshot once for the complete speed-update query.
#[derive(Clone, Copy)]
pub(crate) struct RepeatRateContext<'a> {
    pub(crate) document: &'a ProjectDocument,
    pub(crate) layout: &'a FrozenAudioLayout,
}

type Cuts = BTreeMap<RevisionId, BTreeSet<u64>>;

/// Resolve every distinct historical predicate class among an owner's current
/// uses. Work depends on authored runs, overrides and template boundaries, never
/// the number of plays. Only one live axis is supported; captured historical
/// arguments may still name other, fixed Repeat identities.
pub(crate) fn repeated_lattice_classes(
    current: RepeatRateContext<'_>,
    repeat: &NodeId,
    owner: &NodeId,
    usage: &RepeatOwnerUse,
    template: &AudioPlacementTemplate,
    layout: &FrozenAudioLayout,
    maximum_work: usize,
) -> Result<(Vec<ResolvedAudioPlacement>, usize), DocumentError> {
    let document = current.document;
    let mut work = Work::new(maximum_work)?;
    work.spend(1)?;
    let Some(NodeKind::Repeat {
        child,
        iterations,
        gap,
        ..
    }) = document.nodes().get(repeat).map(|node| &node.kind)
    else {
        return Err(invalid("audio rate axis is not a current Repeat"));
    };
    let play_overrides = document.overrides().get(repeat);
    let gap_overrides = document.gap_overrides().get(repeat);
    let is_gap = matches!(usage, RepeatOwnerUse::DefaultGap);
    let branch = match usage {
        RepeatOwnerUse::DefaultBody => child,
        RepeatOwnerUse::DefaultGap => repeat,
        RepeatOwnerUse::OwnedBody { iteration, root }
        | RepeatOwnerUse::OwnedGap { iteration, root } => {
            work.spend(iterations.segment_count() + 1)?;
            let entries = if matches!(usage, RepeatOwnerUse::OwnedBody { .. }) {
                play_overrides
            } else {
                gap_overrides
            };
            if iterations.position(iteration).is_none()
                || entries.and_then(|entries| entries.get(iteration)) != Some(root)
            {
                return Err(invalid("audio rate owner has a stale Repeat branch"));
            }
            root
        }
    };
    validate_owner(current, repeat, owner, branch, is_gap, &mut work)?;
    template.validate_with(layout, &mut work)?;
    // Node owners can retain a captured historical default-gap recipe after
    // materialization. Its fixed gap argument is not another live axis.
    if is_gap && template.reference.recipe != AudioRecipeKind::RepeatGap {
        return Err(invalid("default gap rate requires a gap lattice"));
    }
    for argument in &template.arguments {
        if let AudioRepeatValue::Live { repeat: axis } = &argument.value
            && (is_gap || axis != repeat)
        {
            return Err(invalid("audio rate classes require one live Repeat axis"));
        }
    }
    for clause in &template.births {
        if is_gap || &clause.repeat != repeat {
            return Err(invalid("audio rate classes require one live Repeat axis"));
        }
    }
    if let Some(AudioRepeatValue::Live { repeat: axis }) = &template.gap_after
        && (!is_gap || axis != repeat)
    {
        return Err(invalid("audio rate classes require the current gap axis"));
    }

    work.spend(iterations.segment_count() * 2)?;
    let final_iteration = iterations
        .len()
        .checked_sub(1)
        .and_then(|index| iterations.at(index))
        .ok_or_else(|| invalid("audio rate axis has no plays"))?;
    let mut resolved = Vec::new();
    match usage {
        RepeatOwnerUse::OwnedBody { iteration, .. }
        | RepeatOwnerUse::OwnedGap { iteration, .. } => {
            if matches!(usage, RepeatOwnerUse::OwnedGap { .. })
                && (iteration == &final_iteration
                    || !positive_subtree(current.layout, branch, &mut work)?)
            {
                return Ok((resolved, work.used));
            }
            resolved.push(resolve(
                template, layout, repeat, owner, iteration, false, &mut work,
            )?);
        }
        RepeatOwnerUse::DefaultBody | RepeatOwnerUse::DefaultGap => {
            if is_gap
                && gap
                    .as_ref()
                    .is_none_or(|gap| gap.duration == FrameDuration::ZERO)
            {
                return Ok((resolved, work.used));
            }
            let exclusions = if is_gap {
                gap_overrides
            } else {
                play_overrides
            };
            let mut cuts = Cuts::new();
            if let Some(exclusions) = exclusions {
                for (iteration, _) in exclusions.iter() {
                    singleton(&mut cuts, iteration, &mut work)?;
                }
            }
            if is_gap {
                singleton(&mut cuts, &final_iteration, &mut work)?;
            }
            historical_cuts(template, layout, &mut cuts, &mut work)?;
            for (allocation, first, count) in iterations.segments() {
                work.spend(1)?;
                let end = u64::from(first) + u64::from(count);
                let mut points = vec![u64::from(first)];
                if let Some(boundaries) = cuts.get(allocation) {
                    for point in boundaries.range((
                        std::ops::Bound::Excluded(u64::from(first)),
                        std::ops::Bound::Excluded(end),
                    )) {
                        work.spend(1)?;
                        points.push(*point);
                    }
                }
                points.push(end);
                for interval in points.windows(2) {
                    work.spend(1)?;
                    let iteration = IterationId {
                        allocation: allocation.clone(),
                        ordinal: u32::try_from(interval[0])
                            .map_err(|_| invalid("audio rate ordinal overflow"))?,
                    };
                    if exclusions.is_some_and(|entries| entries.get(&iteration).is_some())
                        || (is_gap && iteration == final_iteration)
                    {
                        continue;
                    }
                    resolved.push(resolve(
                        template, layout, repeat, owner, &iteration, is_gap, &mut work,
                    )?);
                }
            }
        }
    }
    Ok((resolved, work.used))
}

fn resolve(
    template: &AudioPlacementTemplate,
    layout: &FrozenAudioLayout,
    repeat: &NodeId,
    owner: &NodeId,
    iteration: &IterationId,
    gap: bool,
    work: &mut Work,
) -> Result<ResolvedAudioPlacement, DocumentError> {
    let instance = InstancePath {
        node: owner.clone(),
        repeats: if gap {
            Vec::new()
        } else {
            vec![RepeatInstance {
                node: repeat.clone(),
                iteration: iteration.clone(),
            }]
        },
    };
    template.resolve_with(
        layout,
        if gap {
            AudioBindingEnvironment::GapOccurrence {
                instance: &instance,
                after: iteration,
            }
        } else {
            AudioBindingEnvironment::Occurrence(&instance)
        },
        work,
    )
}

fn singleton(
    cuts: &mut Cuts,
    iteration: &IterationId,
    work: &mut Work,
) -> Result<(), DocumentError> {
    boundary(
        cuts,
        &iteration.allocation,
        u64::from(iteration.ordinal),
        work,
    )?;
    boundary(
        cuts,
        &iteration.allocation,
        u64::from(iteration.ordinal) + 1,
        work,
    )
}

fn boundary(
    cuts: &mut Cuts,
    allocation: &RevisionId,
    point: u64,
    work: &mut Work,
) -> Result<(), DocumentError> {
    work.spend(1)?;
    cuts.entry(allocation.clone()).or_default().insert(point);
    Ok(())
}

fn historical_repeat_cuts(
    layout: &FrozenAudioLayout,
    repeat: &NodeId,
    gap: bool,
    cuts: &mut Cuts,
    work: &mut Work,
) -> Result<(), DocumentError> {
    let Some(FrozenAudioKind::Repeat { iterations, .. }) =
        layout.nodes().get(repeat).map(|node| &node.kind)
    else {
        return Err(invalid("audio rate history is not a Repeat"));
    };
    for (allocation, first, count) in iterations.segments() {
        boundary(cuts, allocation, u64::from(first), work)?;
        boundary(cuts, allocation, u64::from(first) + u64::from(count), work)?;
    }
    let overrides = if gap {
        layout.gap_overrides()
    } else {
        layout.overrides()
    };
    if let Some(entries) = overrides.get(repeat) {
        for (iteration, _) in entries.iter() {
            singleton(cuts, iteration, work)?;
        }
    }
    if gap {
        work.spend(iterations.segment_count() * 2)?;
        if let Some(last) = iterations
            .len()
            .checked_sub(1)
            .and_then(|index| iterations.at(index))
        {
            singleton(cuts, &last, work)?;
        }
    }
    Ok(())
}

fn historical_cuts(
    template: &AudioPlacementTemplate,
    layout: &FrozenAudioLayout,
    cuts: &mut Cuts,
    work: &mut Work,
) -> Result<(), DocumentError> {
    for clause in &template.births {
        match &clause.survivors {
            AudioBirthSurvivors::CapturedRepeat { repeat } => {
                historical_repeat_cuts(layout, repeat, false, cuts, work)?;
            }
            AudioBirthSurvivors::Run {
                allocation,
                first,
                count,
            } => {
                boundary(cuts, allocation, u64::from(*first), work)?;
                boundary(
                    cuts,
                    allocation,
                    u64::from(*first) + u64::from(*count),
                    work,
                )?;
            }
        }
    }
    if matches!(template.gap_after, Some(AudioRepeatValue::Live { .. })) {
        historical_repeat_cuts(layout, &template.reference.physical, true, cuts, work)?;
    }
    Ok(())
}

fn validate_owner(
    current: RepeatRateContext<'_>,
    repeat: &NodeId,
    owner: &NodeId,
    branch: &NodeId,
    gap: bool,
    work: &mut Work,
) -> Result<(), DocumentError> {
    if current.layout.root() != current.document.root() {
        return Err(invalid("audio rate current layout has another root"));
    }
    let (axes, used) =
        current
            .layout
            .scoped_repeats(current.layout.root(), owner, work.remaining()?)?;
    work.spend(used)?;
    if gap {
        if owner != repeat || !axes.is_empty() {
            return Err(invalid("audio rate classes require one live Repeat axis"));
        }
    } else {
        if axes.len() != 1 || axes.first() != Some(repeat) {
            return Err(invalid("audio rate classes require one live Repeat axis"));
        }
        // This indexed lookup proves the declared current branch contains the
        // owner. It does not select a play or reuse historical branch aliases.
        let (_, used) = current
            .layout
            .scale_in_scope(branch, owner, work.remaining()?)?;
        work.spend(used)?;
    }
    Ok(())
}

fn positive_subtree(
    current: &FrozenAudioLayout,
    root: &NodeId,
    work: &mut Work,
) -> Result<bool, DocumentError> {
    work.spend(1)?;
    Ok(current.node_extent(root)?.compare_integer(0).is_gt())
}

#[cfg(test)]
mod tests;
