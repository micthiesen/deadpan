//! One pause in a concrete Repeat branch, with outside-in sparse isolation.

use super::{invalid, limit, overflow};
use crate::{
    AudioTimingId, BeatNode, Command, EditError, ExactRatio, FrameDuration, InsertTimeTarget,
    InstancePath, NodeId, NodeKind, OccurrenceIdentities, ProjectDocument, ProjectFrame,
    SplitIdentities,
};
use serde::{Deserialize, Serialize};

mod prepared;
mod target;
pub use prepared::{PreparedOccurrenceTime, prepare_occurrence_time};
use target::Site;
pub use target::{OccurrencePauseTarget, PauseInsertionTarget, RetimedPauseTarget};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OccurrencePauseIdentities {
    pub isolation: OccurrenceIdentities,
    pub split: SplitIdentities,
    pub container: Option<NodeId>,
    pub gap: Option<NodeId>,
}

pub(crate) struct Prepared {
    pub document: ProjectDocument,
    pub command: Command,
    pub isolation: Vec<crate::ScopedIsolationStep>,
}

/// Isolate before the temporal command captures sounds and placements. The
/// recursive leaf then sees already-owned branches at the same project time.
pub(crate) fn prepare(
    document: &ProjectDocument,
    request: &crate::CommandRequest,
) -> Result<Option<Prepared>, EditError> {
    let Command::InsertOccurrenceTime {
        at,
        within,
        target: authority,
        hold,
        id,
        identities,
        timing,
        ai,
        all_plays,
    } = &request.command
    else {
        return Ok(None);
    };
    if hold.duration == FrameDuration::ZERO || !super::ordinary_hold(hold) {
        return Err(invalid(
            "a pause needs positive duration and an ordinary Hold provider",
        ));
    }
    if *ai
        && (!matches!(
            hold.video,
            crate::HoldVideo::Background | crate::HoldVideo::Freeze { .. }
        ) || hold.audio != crate::HoldAudio::Silence)
    {
        return Err(invalid(
            "an AI pause requires a silent deterministic fallback",
        ));
    }
    if timing.allocation != request.new_revision {
        return Err(invalid(
            "insertion timing allocation must equal the new revision",
        ));
    }
    document
        .duration()?
        .checked_add(hold.duration)
        .map_err(crate::DocumentError::from)?;
    let target = target::for_authority(
        document,
        *at,
        within.as_ref(),
        authority.as_ref(),
        *all_plays,
    )?;
    let shared = is_shared(&target);
    if shared {
        if *ai {
            return Err(invalid(
                "an AI pause needs one play; add it to this play only",
            ));
        }
        if !document.beat_sounds().is_empty() {
            return Err(invalid(
                "a pause for all plays cannot yet move attached sounds; add it to this play only",
            ));
        }
        all_plays_site(document, &target, *at)?;
    }
    if identities.isolation.nodes.len() != target.isolation_nodes
        || identities.isolation.marks.len() != target.isolation_marks
        || identities.split.nodes.len() != target.split_nodes
        || identities.container.is_some() != target.container
        || identities.gap.is_some() != target.gap
    {
        return Err(invalid(
            "occurrence pause requires its exact identity pools",
        ));
    }
    let mut occupied = crate::scoped_edit::occupied_node_ids(document);
    for node in std::iter::once(id)
        .chain(&identities.isolation.nodes)
        .chain(&identities.split.nodes)
        .chain(&identities.container)
        .chain(&identities.gap)
    {
        if !occupied.insert(node) {
            return Err(EditError::new(
                crate::EditErrorCode::IdentityConflict,
                "pause identities must be fresh and distinct, including retained audio names",
            ));
        }
    }
    if target.isolation_nodes == 0 && !target.gap {
        return Ok(None);
    }
    let mut allowances =
        crate::sound_allowance::SoundAllowanceEdit::capture(document, &request.command);
    let mut structural = document.clone();
    structural.sound_allowances.clear();
    let (mut result, mapped_scope, maps) = crate::scoped_edit::isolate_temporal(
        &structural,
        &target.scope(),
        &identities.isolation,
        crate::command::EditContext {
            allocation: &request.new_revision,
            allowances: allowances.as_mut(),
            sound_clocks: None,
        },
    )?;
    let isolation: Vec<_> = maps.into_iter().map(|mapping| mapping.proof()).collect();
    let mut within = if authority.is_some() {
        Some(site_instance(&target.site).clone())
    } else {
        within.clone()
    };
    if let Some(instance) = &mut within {
        for step in &isolation {
            if crate::scoped_edit::matches_prefix(instance, step.before_prefix()) {
                crate::occurrence_edit::remap_instance(instance, step.nodes());
            }
        }
        instance.validate(&result)?;
    }
    let mut timing = timing.clone();
    if let Some(gap) = &identities.gap {
        let target = target::for_command(&result, *at, within.as_ref())?;
        let Site::Gap { repeat, after, .. } = target.site else {
            return Err(invalid("isolated pause lost its default gap"));
        };
        // Materialization changes no root time or root sound route. Its
        // physical binding capture needs the independent bus detached; restore
        // it before the recursive leaf captures the actual insertion ripple.
        let sounds = std::mem::take(&mut result.sounds);
        let routes = std::mem::take(&mut result.sound_routes);
        result = crate::gap_override::isolate(
            &result,
            &repeat.node,
            &after,
            gap,
            &timing,
            &request.new_revision,
        )?;
        result.sounds = sounds;
        result.sound_routes = routes;
        if let Some(allowances) = &mut allowances {
            allowances.explode_gap(&repeat.node, &after, gap)?;
        }
        timing = next_timing(&timing)?;
    }
    if let Some(allowances) = allowances {
        allowances.restore(&mut result)?;
    }
    result.validate()?;
    Ok(Some(Prepared {
        document: result,
        command: Command::InsertOccurrenceTime {
            at: *at,
            within,
            target: authority.as_ref().map(|_| mapped_scope),
            hold: hold.clone(),
            id: id.clone(),
            ai: *ai,
            all_plays: *all_plays,
            timing,
            identities: OccurrencePauseIdentities {
                isolation: OccurrenceIdentities::default(),
                gap: None,
                split: identities.split.clone(),
                container: identities.container.clone(),
            },
        },
        isolation,
    }))
}

pub(crate) fn apply(
    document: &ProjectDocument,
    command: &Command,
    context: crate::command::EditContext<'_>,
) -> Result<ProjectDocument, EditError> {
    let Command::InsertOccurrenceTime {
        at,
        within,
        target: authority,
        hold,
        id,
        identities,
        timing,
        all_plays,
        ..
    } = command
    else {
        return Err(invalid("expected an occurrence pause"));
    };
    let target = target::for_authority(
        document,
        *at,
        within.as_ref(),
        authority.as_ref(),
        *all_plays,
    )?;
    if is_shared(&target) {
        return apply_all_plays(
            document, &target, *at, hold, id, identities, timing, context,
        );
    }
    if target.isolation_nodes != 0 || target.gap {
        return Err(invalid(
            "pause branch must be isolated before placement capture",
        ));
    }
    let allocation = context.allocation;
    let total = document.duration()?.frames();
    let mut working = document.clone();
    let mut placement_timing = timing.clone();
    let (parent, slot) = match target.site {
        Site::Sequence { instance, index } => (instance.node, index),
        Site::Physical {
            instance,
            at: cut,
            sequence,
        } => {
            if target.split_nodes != 0 {
                working.audio_bindings =
                    crate::capture_unbound_audio_bindings(document, timing.clone())?;
                working = super::split_captured(document, &working, |working| {
                    crate::split::apply(working, &instance.node, cut, &identities.split, context)
                })?;
                placement_timing = next_timing(timing)?;
                sequence.map_or_else(
                    || {
                        (
                            identities
                                .split
                                .nodes
                                .last()
                                .expect("exact Split pool")
                                .clone(),
                            1,
                        )
                    },
                    |(parent, index)| (parent, index + 1),
                )
            } else {
                let container = identities
                    .container
                    .as_ref()
                    .ok_or_else(|| invalid("pause needs a Sequence container"))?;
                let parent = working
                    .parent_of(&instance.node)
                    .ok_or_else(|| invalid("pause beat has no parent"))?;
                crate::command::replace_child(
                    &mut working,
                    &parent,
                    &instance.node,
                    container.clone(),
                )?;
                working.nodes.insert(
                    container.clone(),
                    BeatNode::sequence("", vec![instance.node]),
                );
                // Attached sounds retain the pre-container layout under the
                // command's timing identity. The new placement graph needs a
                // separate identity even though this wrapper has no duration.
                placement_timing = next_timing(timing)?;
                (container.clone(), usize::from(cut != FrameDuration::ZERO))
            }
        }
        Site::Gap { .. } => return Err(invalid("pause gap must be materialized before insertion")),
    };
    let affected = super::composite::occurrence_shifted_owners(&working, &parent, slot)?;
    working = super::composite::prepare_owners(&working, &affected, *at, total, &placement_timing)?;
    super::insert_leaf_at(
        &working,
        &parent,
        slot,
        id,
        BeatNode::hold("Pause", hold.clone()),
        allocation,
    )
}

fn site_instance(site: &Site) -> &InstancePath {
    match site {
        Site::Sequence { instance, .. } | Site::Physical { instance, .. } => instance,
        Site::Gap { repeat, .. } => repeat,
    }
}

fn is_shared(target: &OccurrencePauseTarget) -> bool {
    target
        .scope()
        .repeats
        .iter()
        .any(|step| matches!(step.branch, crate::RepeatEditBranch::Default))
}

/// The first Default Repeat, qualified by its concrete Play prefix, and the
/// earliest affected cut in that shared root's old concrete range.
struct AllPlaysSite {
    repeat: NodeId,
    end: i64,
    first_cut: i64,
    /// Default occurrences of the edited definition: the pause appears in
    /// each, so the outermost Repeat grows by this many pause lengths.
    occurrences: i64,
}

/// Admit explicit mixed branches, ordinary Sequences between Repeats, and a
/// cut inside the innermost definition (not an unmaterialized shared gap).
fn all_plays_site(
    document: &ProjectDocument,
    target: &OccurrencePauseTarget,
    at: ProjectFrame,
) -> Result<AllPlaysSite, EditError> {
    let instance = match &target.site {
        Site::Sequence { instance, .. } | Site::Physical { instance, .. } => instance,
        Site::Gap { .. } => {
            return Err(invalid(
                "a pause for all plays goes inside the repeated beat, not in a gap",
            ));
        }
    };
    let scope = target.scope();
    let first_default = scope
        .repeats
        .iter()
        .position(|step| matches!(step.branch, crate::RepeatEditBranch::Default))
        .ok_or_else(|| invalid("a shared pause needs a Default Repeat branch"))?;
    let outer = &instance.repeats[first_default];
    let durations = document.durations()?;
    let index = crate::AnchorIndex::from_durations(document, durations.clone())?;
    let owner = InstancePath {
        node: outer.node.clone(),
        repeats: instance.repeats[..first_default].to_vec(),
    };
    let start = index
        .resolve_target(&crate::AnchorTarget {
            boundary: crate::BoundaryAnchor {
                coordinate: crate::Anchor::Occurrence {
                    instance: owner,
                    position: ExactRatio::ZERO,
                },
                bias: crate::InsertionBias::Right,
            },
            occurrence: None,
        })
        .map_err(|error| invalid(&error.to_string()))?;
    if start.exact_frame.denominator() != 1 {
        return Err(invalid("a shared pause needs an integral Repeat clock"));
    }
    let start = start.frame.0;
    let end = start
        .checked_add(durations[&outer.node].frames())
        .ok_or_else(overflow)?;
    let mut shown = start;
    let mut earliest = start;
    let mut occurrences = 1_i64;
    for (level, step) in instance.repeats.iter().enumerate().skip(first_default) {
        let NodeKind::Repeat { iterations, .. } = &document.nodes()[&step.node].kind else {
            return Err(invalid("expected a Repeat step"));
        };
        let overrides = document.overrides().get(&step.node);
        let layout = index
            .repeats
            .get(&step.node)
            .ok_or_else(|| invalid("the Repeat has no layout"))?;
        let play = layout
            .play(&step.iteration)
            .ok_or_else(|| invalid("the Repeat play is missing"))?;
        let chosen = match &scope.repeats[level].branch {
            crate::RepeatEditBranch::Play { .. } => play.clone(),
            crate::RepeatEditBranch::Default => {
                if overrides.is_some_and(|plays| plays.get(&step.iteration).is_some()) {
                    return Err(invalid(
                        "the shown play does not use the selected default definition",
                    ));
                }
                let count = i64::from(iterations.len())
                    .checked_sub(
                        i64::try_from(overrides.map_or(0, |plays| plays.len()))
                            .map_err(|_| overflow())?,
                    )
                    .ok_or_else(overflow)?;
                occurrences = occurrences.checked_mul(count).ok_or_else(overflow)?;
                (0..iterations.len())
                    .take(overrides.map_or(0, |plays| plays.len()).saturating_add(1))
                    .filter_map(|ordinal| iterations.at(ordinal))
                    .find(|iteration| overrides.is_none_or(|plays| plays.get(iteration).is_none()))
                    .and_then(|iteration| layout.play(&iteration))
                    .ok_or_else(|| invalid("the Repeat has no default play"))?
            }
        };
        shown = shown.checked_add(play.start).ok_or_else(overflow)?;
        earliest = earliest.checked_add(chosen.start).ok_or_else(overflow)?;
        if let Some(next) = instance.repeats.get(level + 1) {
            let mut offset = 0_i64;
            let mut current = &next.node;
            while current != &play.child {
                let (parent, at) = index
                    .parents
                    .get(current)
                    .ok_or_else(|| invalid("the nested Repeat lost its definition"))?;
                if !matches!(document.nodes()[parent].kind, NodeKind::Sequence { .. }) {
                    return Err(invalid(
                        "a shared pause needs ordinary groups between Repeats",
                    ));
                }
                offset = offset.checked_add(*at).ok_or_else(overflow)?;
                current = parent;
            }
            shown = shown.checked_add(offset).ok_or_else(overflow)?;
            earliest = earliest.checked_add(offset).ok_or_else(overflow)?;
        }
    }
    let local = at.0.checked_sub(shown).ok_or_else(overflow)?;
    let first_cut = earliest.checked_add(local).ok_or_else(overflow)?;
    Ok(AllPlaysSite {
        repeat: outer.node.clone(),
        end,
        first_cut,
        occurrences,
    })
}

/// The independent root bus moves as for other Repeat edits: one insertion
/// at the shared root's old end by every affected occurrence's pause.
pub(crate) fn all_plays_root_operation(
    document: &ProjectDocument,
    command: &Command,
) -> Result<crate::RootSoundOperation, EditError> {
    let Command::InsertOccurrenceTime {
        at,
        within,
        hold,
        target: authority,
        all_plays,
        ..
    } = command
    else {
        return Err(invalid("expected an all-plays pause"));
    };
    let target = target::for_authority(
        document,
        *at,
        within.as_ref(),
        authority.as_ref(),
        *all_plays,
    )?;
    let site = all_plays_site(document, &target, *at)?;
    let frames = hold
        .duration
        .frames()
        .checked_mul(site.occurrences)
        .ok_or_else(overflow)?;
    Ok(crate::RootSoundOperation::Insert {
        at: ProjectFrame(site.end),
        duration: FrameDuration::new(frames).map_err(crate::DocumentError::from)?,
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "the destructured command fields of one leaf"
)]
fn apply_all_plays(
    document: &ProjectDocument,
    target: &OccurrencePauseTarget,
    at: ProjectFrame,
    hold: &crate::HoldRecipe,
    id: &NodeId,
    identities: &OccurrencePauseIdentities,
    timing: &AudioTimingId,
    context: crate::command::EditContext<'_>,
) -> Result<ProjectDocument, EditError> {
    let site = all_plays_site(document, target, at)?;
    let total = document.duration()?.frames();
    let first_cut = site.first_cut;
    let allocation = context.allocation;
    // Every edit retains unbound lattices first, as gap changes do.
    let mut working = document.clone();
    working.audio_bindings = crate::capture_unbound_audio_bindings(document, timing.clone())?;
    let mut next = next_timing(timing)?;
    let (parent, slot) = match &target.site {
        Site::Sequence { instance, index } => (instance.node.clone(), *index),
        Site::Physical {
            instance,
            at: cut,
            sequence,
        } => {
            if target.split_nodes != 0 {
                working = super::split_captured(document, &working, |working| {
                    crate::split::apply(working, &instance.node, *cut, &identities.split, context)
                })?;
                sequence.clone().map_or_else(
                    || {
                        (
                            identities
                                .split
                                .nodes
                                .last()
                                .expect("exact Split pool")
                                .clone(),
                            1,
                        )
                    },
                    |(parent, index)| (parent, index + 1),
                )
            } else {
                let container = identities
                    .container
                    .as_ref()
                    .ok_or_else(|| invalid("pause needs a Sequence container"))?;
                let parent = working
                    .parent_of(&instance.node)
                    .ok_or_else(|| invalid("pause beat has no parent"))?;
                crate::command::replace_child(
                    &mut working,
                    &parent,
                    &instance.node,
                    container.clone(),
                )?;
                working.nodes.insert(
                    container.clone(),
                    BeatNode::sequence("", vec![instance.node.clone()]),
                );
                (container.clone(), usize::from(*cut != FrameDuration::ZERO))
            }
        }
        Site::Gap { .. } => unreachable!("admitted above"),
    };
    // Every default play grows; each owner inside the Repeat from the first
    // default cut keeps its own pre-edit clock, then the suffix after it.
    if first_cut < site.end {
        working = super::composite::prepare_owners(
            &working,
            &super::composite::repeat_interior_owners(&working, &site.repeat)?,
            ProjectFrame(first_cut),
            site.end,
            &next,
        )?;
        next = next_timing(&next)?;
    }
    if site.end < total {
        working = super::composite::prepare_owners(
            &working,
            &super::composite::occurrence_owners_after(&working, &site.repeat)?,
            ProjectFrame(site.end),
            total,
            &next,
        )?;
    }
    super::insert_leaf_at(
        &working,
        &parent,
        slot,
        id,
        BeatNode::hold("Pause", hold.clone()),
        allocation,
    )
}

fn next_timing(timing: &AudioTimingId) -> Result<AudioTimingId, EditError> {
    Ok(AudioTimingId {
        allocation: timing.allocation.clone(),
        ordinal: timing
            .ordinal
            .checked_add(1)
            .ok_or_else(|| limit("pause timing identity overflow"))?,
    })
}

#[cfg(test)]
mod tests;
