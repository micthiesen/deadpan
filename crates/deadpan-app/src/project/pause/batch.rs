//! Resolve one uniform selection of Repeat plays into one durable pause edit.

use std::{collections::BTreeSet, sync::Arc};

use deadpan_core::{
    Anchor, AnchorIndex, AnchorTarget, BoundaryAnchor, Command, CommandRequest, ExactRatio,
    FrameDuration, InsertionBias, InstancePath, IterationId, LeafEdit, MAX_COMPOUND_STEPS,
    MAX_SCOPED_TARGETS, NodeKind, ProjectDocument, ProjectFrame, RepeatEditBranch, RepeatInstance,
    ResolvedBoundary, ResolvedStep, ResolvedTransaction, ScopedNodeTarget,
};
use deadpan_plan::RenderPlan;

use super::super::{Workspace, scoped};

#[cfg(test)]
mod tests;

pub(in crate::project) struct Prepared {
    pub request: CommandRequest,
    pub document: ProjectDocument,
    pub target: ScopedNodeTarget,
    /// Actual inserted Holds in the other selected plays. Cuts of corresponding
    /// Sequence owners can land in different children, so child-slot retargeting
    /// cannot reconstruct this set after insertion.
    pub also: Vec<ScopedNodeTarget>,
    pub presentation: InstancePath,
    pub boundary: ResolvedBoundary,
}

struct Site {
    target: ScopedNodeTarget,
    presentation: InstancePath,
    local: ExactRatio,
}

/// `also` must vary one common Repeat play under the same exact prefix.
/// Each selected owner retains its own deeper occurrence choices and local cut.
/// Concrete descendant slots may differ after earlier pauses split children.
/// Mere disjointness without that common prefix is insufficient:
/// mixing an outer Play with an outer Default can miss an earlier cloned branch.
pub(in crate::project) fn prepare(
    workspace: &Workspace,
    target: &scoped::Target,
    duration: FrameDuration,
    black: bool,
    ai: bool,
    bank_version: u64,
) -> Result<Prepared, String> {
    let count = target
        .also
        .len()
        .checked_add(1)
        .ok_or("Selected pause count overflowed.")?;
    if !(2..=MAX_SCOPED_TARGETS).contains(&count) || count > MAX_COMPOUND_STEPS {
        return Err("A pause batch needs 2..=1024 selected plays.".into());
    }
    if ai {
        return Err(
            "An AI pause needs one play. Choose This play (:scope play N), then insert it.".into(),
        );
    }
    target.validate(workspace)?;
    let document = &workspace.document;
    let varied = uniform_level(document, target)?;
    let iterations = std::iter::once(&target.target)
        .chain(&target.also)
        .map(|other| match &other.repeats[varied].branch {
            RepeatEditBranch::Play { iteration } => Ok(iteration.clone()),
            RepeatEditBranch::Default => Err("Selected play authority is missing.".to_owned()),
        })
        .collect::<Result<Vec<_>, String>>()?;
    let primary = target
        .presentation
        .as_ref()
        .ok_or("Selected plays need a visible occurrence for a pause.")?;
    document
        .pause_insertion_scoped(target.cursor, &target.target, primary)
        .map_err(|error| error.to_string())?;
    let start = resolve(document, primary, ExactRatio::ZERO)?;
    integral(&start)?;
    let local = ExactRatio::integer(target.cursor.0)
        .checked_sub(start.exact_frame)
        .map_err(|error| error.to_string())?;
    if local.denominator() != 1 || local.numerator() < 0 {
        return Err("Selected pause needs an exact local frame boundary.".into());
    }
    let mut sites = Vec::with_capacity(count);
    sites.push(Site {
        target: target.target.clone(),
        presentation: primary.clone(),
        local,
    });
    for other in &target.also {
        sites.push(Site {
            target: other.clone(),
            presentation: presentation(document, other, primary)?,
            local,
        });
    }
    // Admit every captured original boundary before preparing any staged edit.
    // This also refuses dormant Default definitions and unavailable time clocks.
    for (index, site) in sites.iter().enumerate() {
        let at = integral(&resolve(document, &site.presentation, site.local)?)?;
        if index == 0 && at != target.cursor {
            return Err("Selected pause boundary cannot be transferred in this time clock.".into());
        }
        document
            .pause_insertion_scoped(at, &site.target, &site.presentation)
            .map_err(|error| error.to_string())?;
    }

    let mut staged = workspace.annotated();
    let mut holds: Vec<Site> = Vec::with_capacity(count);
    let mut steps = Vec::with_capacity(count);
    for index in 0..count {
        let before = Arc::clone(&staged.document);
        let site = &sites[index];
        let at = integral(&resolve(&before, &site.presentation, site.local)?)?;
        let captured = scoped::Target {
            revision: before.revision_id().clone(),
            target: site.target.clone(),
            presentation: Some(site.presentation.clone()),
            cursor: at,
            also: Vec::new(),
            ..target.clone()
        };
        // Each play samples its own qualified fallback and captured framing.
        let request = super::prepare_scoped(&staged, &captured, duration, black, false)?;
        let prepared = deadpan_core::prepare_occurrence_time(&before, &request)
            .map_err(|error| error.to_string())?;
        for retained in sites.iter_mut().skip(index + 1).chain(&mut holds) {
            retained.target = prepared
                .map_target(&before, &retained.target)
                .map_err(|error| error.to_string())?;
            retained.presentation = prepared
                .map_instance(&before, &retained.presentation)
                .map_err(|error| error.to_string())?;
        }
        holds.push(Site {
            target: prepared.target,
            presentation: prepared.presentation,
            local: ExactRatio::ZERO,
        });
        steps.push(ResolvedStep::Edit {
            edit: LeafEdit::new(request.new_revision, request.command)
                .map_err(|error| error.to_string())?,
        });
        let after = prepared.document;
        staged.plan = Arc::new(RenderPlan::compile(&after).map_err(|error| error.to_string())?);
        staged.clocked_retimes = Arc::new(after.retimes_with_derived_clocks());
        staged.document = Arc::new(after);
    }
    let request = CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: super::super::service::revision(),
        command: Command::Compound {
            transaction: ResolvedTransaction::new(bank_version, Default::default(), steps)
                .map_err(|error| error.to_string())?,
        },
    };
    // Preview the actual Compound, including its final revision and aggregate
    // resource limits, rather than treating the last staged leaf as committed.
    let (_, document) =
        deadpan_core::apply_with_result(document, &request).map_err(|error| error.to_string())?;
    let primary = holds.first().ok_or("Selected pause batch is empty.")?;
    for (hold, iteration) in holds.iter().zip(&iterations) {
        if !hold
            .target
            .matches_instance(&document, &hold.presentation)
            .map_err(|error| error.to_string())?
        {
            return Err("A selected pause lost its captured occurrence.".into());
        }
        let Some(step) = hold.target.repeats.get(varied) else {
            return Err("A selected pause lost its captured Repeat level.".into());
        };
        if !matches!(&step.branch, RepeatEditBranch::Play { iteration: selected } if selected == iteration)
            || hold
                .presentation
                .repeats
                .get(varied)
                .is_none_or(|shown| &shown.iteration != iteration || shown.node != step.repeat)
        {
            return Err("A selected pause lost its captured play identity.".into());
        }
    }
    let boundary = resolve(&document, &primary.presentation, ExactRatio::ZERO)?;
    integral(&boundary)?;
    Ok(Prepared {
        request,
        document,
        target: primary.target.clone(),
        also: holds
            .iter()
            .skip(1)
            .map(|hold| hold.target.clone())
            .collect(),
        presentation: primary.presentation.clone(),
        boundary,
    })
}

fn uniform_level(document: &ProjectDocument, target: &scoped::Target) -> Result<usize, String> {
    for selected in std::iter::once(&target.target).chain(&target.also) {
        selected
            .validate(document)
            .map_err(|error| error.to_string())?;
    }
    for (index, step) in target.target.repeats.iter().enumerate() {
        let RepeatEditBranch::Play { iteration } = &step.branch else {
            continue;
        };
        let mut selected = BTreeSet::from([iteration.clone()]);
        let uniform = target.also.iter().all(|other| {
            let Some(other_step) = other.repeats.get(index) else {
                return false;
            };
            let RepeatEditBranch::Play { iteration } = &other_step.branch else {
                return false;
            };
            other_step.repeat == step.repeat
                && selected.insert(iteration.clone())
                && other.repeats[..index] == target.target.repeats[..index]
        });
        if uniform {
            return Ok(index);
        }
    }
    Err("Selected pauses need distinct plays of one Repeat under the same captured prefix.".into())
}

fn presentation(
    document: &ProjectDocument,
    target: &ScopedNodeTarget,
    primary: &InstancePath,
) -> Result<InstancePath, String> {
    let repeats = target
        .repeats
        .iter()
        .enumerate()
        .map(|(index, step)| {
            let iteration = match &step.branch {
                RepeatEditBranch::Play { iteration } => iteration.clone(),
                RepeatEditBranch::Default => {
                    let NodeKind::Repeat { iterations, .. } = &document.nodes()[&step.repeat].kind
                    else {
                        return Err("Selected pause ancestor is not a Repeat.".to_owned());
                    };
                    let overrides = document.overrides().get(&step.repeat);
                    let visible = |iteration: &IterationId| {
                        iterations.position(iteration).is_some()
                            && overrides.is_none_or(|entries| entries.get(iteration).is_none())
                    };
                    if let Some(preferred) = primary.repeats.get(index)
                        && visible(&preferred.iteration)
                    {
                        preferred.iteration.clone()
                    } else {
                        // At most overrides+1 candidates can be needed to find
                        // a default use, regardless of the Repeat's play count.
                        let bound = overrides.map_or(0, |entries| entries.len());
                        (0..=bound)
                            .filter_map(|position| u32::try_from(position).ok())
                            .filter_map(|position| iterations.at(position))
                            .find(visible)
                            .ok_or("Selected Default scope has no visible play for a pause.")?
                    }
                }
            };
            Ok(RepeatInstance {
                node: step.repeat.clone(),
                iteration,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let instance = InstancePath {
        node: target.node.clone(),
        repeats,
    };
    if !target
        .matches_instance(document, &instance)
        .map_err(|error| error.to_string())?
    {
        return Err("Selected pause has no matching visible occurrence.".into());
    }
    Ok(instance)
}

fn resolve(
    document: &ProjectDocument,
    instance: &InstancePath,
    local: ExactRatio,
) -> Result<ResolvedBoundary, String> {
    AnchorIndex::new(document)
        .map_err(|error| error.to_string())?
        .resolve_target(&AnchorTarget {
            boundary: BoundaryAnchor {
                coordinate: Anchor::Occurrence {
                    instance: instance.clone(),
                    position: local,
                },
                bias: InsertionBias::Right,
            },
            occurrence: None,
        })
        .map_err(|error| error.to_string())
}

fn integral(boundary: &ResolvedBoundary) -> Result<ProjectFrame, String> {
    if boundary.exact_frame != ExactRatio::integer(boundary.frame.0) {
        return Err("Selected pause boundary does not land on an exact project frame.".into());
    }
    Ok(boundary.frame)
}
