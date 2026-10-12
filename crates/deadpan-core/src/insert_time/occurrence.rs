//! One pause in a concrete Repeat branch, with outside-in sparse isolation.

use super::{invalid, limit, overflow};
use crate::{
    AudioTimingId, BeatNode, Command, EditError, ExactRatio, FrameDuration, InsertTimeTarget,
    InstancePath, NodeId, NodeKind, OccurrenceIdentities, ProjectDocument, ProjectFrame,
    SplitIdentities,
};
use serde::{Deserialize, Serialize};

mod target;
use target::Site;
pub use target::{OccurrencePauseTarget, PauseInsertionTarget};

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
        hold,
        id,
        identities,
        timing,
        ai,
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
    let PauseInsertionTarget::Occurrence(target) = document.pause_insertion_target(*at)? else {
        return Err(invalid("occurrence pause requires a Repeat interior"));
    };
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
    let (mut result, _, maps) = crate::scoped_edit::isolate_temporal(
        &structural,
        &target.scope(),
        &identities.isolation,
        crate::command::EditContext {
            allocation: &request.new_revision,
            allowances: allowances.as_mut(),
            sound_clocks: None,
        },
    )?;
    let mut timing = timing.clone();
    if let Some(gap) = &identities.gap {
        let PauseInsertionTarget::Occurrence(target) = result.pause_insertion_target(*at)? else {
            return Err(invalid("isolated pause lost its Repeat placement"));
        };
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
            hold: hold.clone(),
            id: id.clone(),
            ai: *ai,
            timing,
            identities: OccurrencePauseIdentities {
                isolation: OccurrenceIdentities::default(),
                gap: None,
                split: identities.split.clone(),
                container: identities.container.clone(),
            },
        },
        isolation: maps.into_iter().map(|mapping| mapping.proof()).collect(),
    }))
}

pub(crate) fn apply(
    document: &ProjectDocument,
    command: &Command,
    context: crate::command::EditContext<'_>,
) -> Result<ProjectDocument, EditError> {
    let Command::InsertOccurrenceTime {
        at,
        hold,
        id,
        identities,
        timing,
        ..
    } = command
    else {
        return Err(invalid("expected an occurrence pause"));
    };
    let PauseInsertionTarget::Occurrence(target) = document.pause_insertion_target(*at)? else {
        return Err(invalid("occurrence pause requires a Repeat interior"));
    };
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
