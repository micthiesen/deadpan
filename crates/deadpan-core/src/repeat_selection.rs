//! Atomic Repeat authoring in an ordinary Sequence. Endpoint contexts remain
//! editable, original clocks survive, and the root sound bus moves only once.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::insert_time::{composite, sequence_range};
use crate::{
    AudioEdgePolicies, AudioTimingId, BeatNode, Command, DocumentError, EditError, EditErrorCode,
    FrameDuration, FrameRange, HoldRecipe, IterationId, IterationOrder, MAX_DOCUMENT_NODES, NodeId,
    NodeKind, ProjectDocument, ProjectFrame, RepeatLayout, RevisionId, RootSoundOperation,
    SliceCaptureSelection, SplitIdentities, Subtree, WrapAnchorPolicy,
};

#[cfg(test)]
mod tests;

/// Exact caller-owned pools. A range needs one neutral body Sequence; an exact
/// child stays the literal Repeat child and therefore forbids a group identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepeatSelectionIdentities {
    pub repeat: NodeId,
    pub group: Option<NodeId>,
    pub split: SplitIdentities,
}

/// An independent Hold that replaces the rendered gap after one stable play.
/// The caller supplies its fresh node identity; the recipe is ordinary data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepeatGapHold {
    pub after: IterationId,
    pub id: NodeId,
    pub hold: HoldRecipe,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepeatSelectionPlan {
    pub range: FrameRange,
    pub required_split_ids: usize,
    pub needs_group: bool,
    /// Duration of the new Repeat, not the entire project.
    pub output_duration: FrameDuration,
}

impl ProjectDocument {
    pub fn repeat_selection(
        &self,
        parent: &NodeId,
        selection: &SliceCaptureSelection,
        plays: u32,
    ) -> Result<RepeatSelectionPlan, EditError> {
        let (range, required_split_ids, needs_group) = match selection {
            SliceCaptureSelection::Child { node } => {
                let (_, range) = child_range(self, parent, node)?;
                if self.nodes().len() == MAX_DOCUMENT_NODES {
                    return Err(limit("Repeat wrapper exceeds the document node limit"));
                }
                (range, 0, false)
            }
            SliceCaptureSelection::Range { range } => {
                let resolved = sequence_range::preflight_repeat(self, parent, *range)?;
                (*range, resolved.required_ids, true)
            }
            SliceCaptureSelection::Children { first, last } => {
                let selected = self.sequence_children(parent, first, last)?;
                if self.nodes().len().saturating_add(2) > MAX_DOCUMENT_NODES {
                    return Err(limit("Repeat forest exceeds the document node limit"));
                }
                (selected.range, 0, true)
            }
        };
        let output_duration = crate::repeat_duration(range.duration(), plays, FrameDuration::ZERO)
            .map_err(DocumentError::from)?;
        checked_total(self, range.duration(), output_duration)?;
        Ok(RepeatSelectionPlan {
            range,
            required_split_ids,
            needs_group,
            output_duration,
        })
    }
}

struct PlaysPlan {
    parent: NodeId,
    slot: usize,
    range: FrameRange,
    output_duration: FrameDuration,
}

/// Resolve a count without creating or expanding any iteration identities.
fn plays_plan(
    document: &ProjectDocument,
    node: &NodeId,
    plays: u32,
) -> Result<PlaysPlan, EditError> {
    if plays == 0 {
        return Err(invalid("Repeat total plays must be positive"));
    }
    let parent = document
        .parent_of(node)
        .ok_or_else(|| unavailable("Repeat needs a Sequence parent"))?;
    let (slot, range) = child_range(document, &parent, node)?;
    let NodeKind::Repeat {
        child,
        iterations,
        gap,
        ..
    } = &document.nodes()[node].kind
    else {
        return Err(EditError::new(
            EditErrorCode::WrongNodeKind,
            "set-repeat-plays requires an existing Repeat",
        ));
    };
    let durations = document.durations()?;
    let gap_duration = gap.as_ref().map_or(FrameDuration::ZERO, |gap| gap.duration);
    let layout = RepeatLayout::compile_with_gap_overrides(
        iterations,
        child,
        document.overrides().get(node),
        gap_duration,
        document.gap_overrides().get(node),
        &durations,
    )?;
    let output_duration = if plays <= iterations.len() {
        let play = layout
            .play(&iterations.at(plays - 1).expect("positive retained play"))
            .expect("validated Repeat layout");
        FrameDuration::new(
            play.start
                .checked_add(play.duration.frames())
                .ok_or_else(overflow)?,
        )
        .map_err(DocumentError::from)?
    } else {
        let last = iterations
            .at(iterations.len() - 1)
            .expect("nonempty Repeat");
        let activated_gap = document
            .gap_overrides()
            .get(node)
            .and_then(|entries| entries.get(&last))
            .map_or(gap_duration, |root| durations[root]);
        let added =
            crate::repeat_duration(durations[child], plays - iterations.len(), gap_duration)
                .map_err(DocumentError::from)?;
        FrameDuration::new(
            layout
                .duration()
                .frames()
                .checked_add(activated_gap.frames())
                .and_then(|value| value.checked_add(added.frames()))
                .ok_or_else(overflow)?,
        )
        .map_err(DocumentError::from)?
    };
    checked_total(document, range.duration(), output_duration)?;
    Ok(PlaysPlan {
        parent,
        slot,
        range,
        output_duration,
    })
}

pub(crate) fn root_operation(
    document: &ProjectDocument,
    command: &Command,
) -> Result<Option<RootSoundOperation>, EditError> {
    let (range, duration) = match command {
        Command::RepeatSelection {
            parent,
            selection,
            plays,
            ..
        } => {
            let plan = document.repeat_selection(parent, selection, *plays)?;
            (plan.range, plan.output_duration)
        }
        Command::SetRepeatPlays { node, plays, .. } => {
            let plan = plays_plan(document, node, *plays)?;
            (plan.range, plan.output_duration)
        }
        Command::SetRepeatGaps {
            node,
            gap,
            branches,
            timing,
        } => {
            let (plan, _) = gaps_plan(document, node, gap.as_ref(), branches, &timing.allocation)?;
            (plan.range, plan.output_duration)
        }
        _ => return Err(invalid("expected a retained-clock Repeat command")),
    };
    let new_end = ProjectFrame(
        range
            .start()
            .0
            .checked_add(duration.frames())
            .ok_or_else(overflow)?,
    );
    Ok(if new_end > range.end() {
        Some(RootSoundOperation::Insert {
            at: range.end(),
            duration: FrameDuration::new(new_end.0 - range.end().0).map_err(DocumentError::from)?,
        })
    } else if new_end < range.end() {
        Some(RootSoundOperation::Delete {
            range: FrameRange::new(new_end, range.end()).map_err(DocumentError::from)?,
        })
    } else {
        None
    })
}

pub(crate) fn apply(
    document: &ProjectDocument,
    command: &Command,
    context: crate::command::EditContext<'_>,
) -> Result<ProjectDocument, EditError> {
    match command {
        Command::RepeatSelection {
            parent,
            selection,
            plays,
            identities,
            timing,
        } => wrap(
            document, parent, selection, *plays, identities, timing, context,
        ),
        Command::SetRepeatPlays {
            node,
            plays,
            timing,
        } => set_plays(document, node, *plays, timing, context),
        Command::SetRepeatGaps {
            node,
            gap,
            branches,
            timing,
        } => set_gaps(document, node, gap.as_ref(), branches, timing, context),
        _ => Err(invalid("expected a retained-clock Repeat command")),
    }
}

fn wrap(
    document: &ProjectDocument,
    parent: &NodeId,
    selection: &SliceCaptureSelection,
    plays: u32,
    identities: &RepeatSelectionIdentities,
    timing: &AudioTimingId,
    mut context: crate::command::EditContext<'_>,
) -> Result<ProjectDocument, EditError> {
    let plan = document.repeat_selection(parent, selection, plays)?;
    validate_timing(document, timing, context.allocation)?;
    validate_identities(document, identities, &plan)?;
    let mut working = document.clone();
    // Even a whole child needs its pre-wrap lattice: the first play must retain
    // the old root phase, and established inner birth scopes remain intrinsic.
    working.audio_bindings = crate::capture_unbound_audio_bindings(document, timing.clone())?;
    let (mut working, consumed) = sequence_range::split_prepared(
        &working,
        parent,
        &[plan.range.start(), plan.range.end()],
        &identities.split,
        crate::command::EditContext {
            allocation: context.allocation,
            allowances: context.allowances.as_deref_mut(),
        },
    )?;
    if consumed != plan.required_split_ids {
        return Err(invalid("Repeat Split consumption differs from preflight"));
    }
    let (first, end, selected) = match selection {
        SliceCaptureSelection::Child { node } => {
            let (slot, _) = child_range(&working, parent, node)?;
            (slot, slot + 1, vec![node.clone()])
        }
        SliceCaptureSelection::Range { .. } => {
            let selected = sequence_range::selected_children(&working, parent, plan.range)?;
            (selected.first, selected.end, selected.nodes)
        }
        SliceCaptureSelection::Children { first, last } => {
            let selected = working.sequence_children(parent, first, last)?;
            let NodeKind::Sequence { children } = &working.nodes()[parent].kind else {
                unreachable!("exact child query admitted a Sequence")
            };
            (
                selected.first,
                selected.end,
                children[selected.first..selected.end].to_vec(),
            )
        }
    };
    if plays > 1 && plan.range.end().0 < document.duration()?.frames() {
        let suffix_timing = next_timing(&working, timing)?;
        working = composite::prepare_suffix(
            &working,
            parent,
            end,
            plan.range.end(),
            document.duration()?.frames(),
            &suffix_timing,
        )?;
    }
    let mut result = working.clone();
    let child = if let Some(group) = &identities.group {
        result.nodes.insert(
            group.clone(),
            BeatNode::sequence("Repeated range", selected.clone()),
        );
        group.clone()
    } else {
        selected[0].clone()
    };
    let iterations = IterationOrder::new(context.allocation.clone(), plays)?;
    let first_play = iterations.at(0).expect("positive Repeat plays");
    let mut repeat = BeatNode::sequence("Repeat", Vec::new());
    repeat.kind = NodeKind::Repeat {
        child,
        iterations,
        gap: None,
        escalation: None,
    };
    result.nodes.insert(identities.repeat.clone(), repeat);
    let NodeKind::Sequence { children } =
        &mut result.nodes.get_mut(parent).expect("admitted parent").kind
    else {
        unreachable!()
    };
    children.splice(first..end, [identities.repeat.clone()]);
    if let Some(allowances) = context.allowances {
        let mut owned = BTreeSet::new();
        for child in &selected {
            owned.extend(crate::occurrence_edit::subtree_order(&working, child)?);
        }
        allowances.wrap_repeat(&owned, &identities.repeat, &first_play)?;
    }
    // The existing mark transform expresses first-play provenance, including
    // concrete inner occurrences. The synthetic command is never persisted.
    let mark_command = Command::WrapRepeat {
        node: selected[0].clone(),
        id: identities.repeat.clone(),
        plays,
        gap: None,
        anchor_policy: WrapAnchorPolicy::First,
    };
    finish(&working, result, &mark_command)
}

fn set_plays(
    document: &ProjectDocument,
    node: &NodeId,
    plays: u32,
    timing: &AudioTimingId,
    context: crate::command::EditContext<'_>,
) -> Result<ProjectDocument, EditError> {
    let plan = plays_plan(document, node, plays)?;
    validate_timing(document, timing, context.allocation)?;
    let NodeKind::Repeat {
        iterations, gap, ..
    } = &document.nodes()[node].kind
    else {
        unreachable!()
    };
    if plays == iterations.len() {
        return Ok(document.clone());
    }
    let mut working = document.clone();
    working.audio_bindings = crate::capture_unbound_audio_bindings(document, timing.clone())?;
    if plan.output_duration != plan.range.duration()
        && plan.range.end().0 < document.duration()?.frames()
    {
        let suffix_timing = next_timing(&working, timing)?;
        working = composite::prepare_suffix(
            &working,
            &plan.parent,
            plan.slot + 1,
            plan.range.end(),
            document.duration()?.frames(),
            &suffix_timing,
        )?;
    }
    let mut result = working.clone();
    let command = Command::SetRepeat {
        node: node.clone(),
        plays,
        gap: gap.clone(),
    };
    crate::command::reduce(&mut result, &command, context.allocation)?;
    if let Some(allowances) = context.allowances {
        let NodeKind::Repeat { iterations, .. } = &result.nodes()[node].kind else {
            unreachable!()
        };
        allowances.resize_repeat(node, iterations)?;
    }
    finish(&working, result, &command)
}

/// Validate the gap request and resolve the Repeat's new output duration by
/// reducing a trial copy. Branches name distinct plays that are followed by a
/// rendered gap; a final play's dormant gap is not a target.
fn gaps_plan(
    document: &ProjectDocument,
    node: &NodeId,
    gap: Option<&HoldRecipe>,
    branches: &[RepeatGapHold],
    allocation: &RevisionId,
) -> Result<(PlaysPlan, ProjectDocument), EditError> {
    let parent = document
        .parent_of(node)
        .ok_or_else(|| unavailable("Repeat needs a Sequence parent"))?;
    let (slot, range) = child_range(document, &parent, node)?;
    let NodeKind::Repeat { iterations, .. } = &document.nodes()[node].kind else {
        return Err(EditError::new(
            EditErrorCode::WrongNodeKind,
            "set-repeat-gaps requires an existing Repeat",
        ));
    };
    if !u32::try_from(branches.len()).is_ok_and(|count| count < iterations.len()) {
        return Err(invalid("a Repeat has one fewer gap than plays"));
    }
    let mut used: BTreeSet<_> = document.nodes().keys().collect();
    used.extend(document.audio_lineage().values().map(|value| &value.origin));
    for layout in document.audio_bindings().timings.values() {
        used.extend(layout.nodes().keys());
        used.extend(layout.audio_lineage().values().map(|value| &value.origin));
    }
    let mut seen = BTreeSet::new();
    for branch in branches {
        if !seen.insert(&branch.after) {
            return Err(invalid("each gap can be replaced once"));
        }
        if iterations
            .position(&branch.after)
            .is_none_or(|position| position + 1 >= iterations.len())
        {
            return Err(unavailable(
                "a replaced gap must follow a play that is not the last",
            ));
        }
        if branch.hold.duration == FrameDuration::ZERO {
            return Err(invalid("a replaced gap needs a positive duration"));
        }
        if !used.insert(&branch.id) {
            return Err(EditError::new(
                EditErrorCode::IdentityConflict,
                "gap Hold identities must be fresh and distinct",
            ));
        }
    }
    if gap.is_some_and(|gap| gap.duration == FrameDuration::ZERO) {
        return Err(invalid(
            "a default gap needs a positive duration; omit it to remove the gap",
        ));
    }
    // Only the structural duration matters here. Sound relations transform in
    // the enclosing transaction, so they must not constrain the trial tree.
    let mut trial = document.clone();
    trial.sounds.clear();
    trial.sound_routes.clear();
    trial.sound_allowances.clear();
    reduce_gaps(&mut trial, node, gap, branches, allocation)?;
    crate::audio_binding_lifecycle::prune(&mut trial);
    let output_duration = trial.durations()?[node];
    checked_total(document, range.duration(), output_duration)?;
    Ok((
        PlaysPlan {
            parent,
            slot,
            range,
            output_duration,
        },
        trial,
    ))
}

/// The plain structural change: the default gap, then each independent Hold.
fn reduce_gaps(
    document: &mut ProjectDocument,
    node: &NodeId,
    gap: Option<&HoldRecipe>,
    branches: &[RepeatGapHold],
    allocation: &RevisionId,
) -> Result<(), EditError> {
    let NodeKind::Repeat { iterations, .. } = &document.nodes()[node].kind else {
        unreachable!("gap plan admitted a Repeat")
    };
    let plays = iterations.len();
    let edges = AudioEdgePolicies {
        node_start: document.nodes()[node].audio_edges.repeat_gap_start,
        node_end: document.nodes()[node].audio_edges.repeat_gap_end,
        ..Default::default()
    };
    crate::command::reduce(
        document,
        &Command::SetRepeat {
            node: node.clone(),
            plays,
            gap: gap.cloned(),
        },
        allocation,
    )?;
    // The request is the complete gap set: branches it does not name end,
    // including a final play's dormant one, and expose the default gap.
    let unnamed: Vec<_> = document
        .gap_overrides()
        .get(node)
        .into_iter()
        .flat_map(|entries| entries.iter())
        .map(|(after, _)| after.clone())
        .filter(|after| branches.iter().all(|branch| &branch.after != after))
        .collect();
    for after in unnamed {
        crate::command::reduce(
            document,
            &Command::ClearGapOverride {
                node: node.clone(),
                iteration: after,
            },
            allocation,
        )?;
    }
    for branch in branches {
        let mut hold = BeatNode::hold("Gap", branch.hold.clone());
        hold.audio_edges = edges;
        crate::command::reduce(
            document,
            &Command::SetGapOverride {
                node: node.clone(),
                iteration: branch.after.clone(),
                subtree: Subtree {
                    root: branch.id.clone(),
                    nodes: [(branch.id.clone(), hold)].into(),
                    overrides: Default::default(),
                    gap_overrides: Default::default(),
                },
            },
            allocation,
        )?;
    }
    Ok(())
}

/// Absolute end of `repeat`'s first play, which no gap change can move.
fn first_play_end(
    document: &ProjectDocument,
    repeat: &NodeId,
    start: ProjectFrame,
) -> Result<ProjectFrame, EditError> {
    let NodeKind::Repeat {
        child,
        iterations,
        gap,
        ..
    } = &document.nodes()[repeat].kind
    else {
        unreachable!("gap plan admitted a Repeat")
    };
    let durations = document.durations()?;
    let layout = RepeatLayout::compile_with_gap_overrides(
        iterations,
        child,
        document.overrides().get(repeat),
        gap.as_ref().map_or(FrameDuration::ZERO, |gap| gap.duration),
        document.gap_overrides().get(repeat),
        &durations,
    )?;
    let first = iterations.at(0).expect("positive Repeat plays");
    let play = layout.play(&first).expect("validated Repeat layout");
    start
        .0
        .checked_add(play.start)
        .and_then(|value| value.checked_add(play.duration.frames()))
        .map(ProjectFrame)
        .ok_or_else(overflow)
}

impl ProjectDocument {
    /// Whether `SetRepeatGaps` would leave `node`'s gaps exactly as they are:
    /// the same default recipe and, after each named play, an existing
    /// independent Hold with the same recipe, with no other branch.
    pub fn repeat_gaps_unchanged(
        &self,
        node: &NodeId,
        gap: Option<&HoldRecipe>,
        branches: &[(IterationId, HoldRecipe)],
    ) -> bool {
        let Some(NodeKind::Repeat { gap: current, .. }) = self.nodes().get(node).map(|n| &n.kind)
        else {
            return false;
        };
        let existing: Vec<_> = self
            .gap_overrides()
            .get(node)
            .into_iter()
            .flat_map(|entries| entries.iter())
            .collect();
        current.as_ref() == gap
            && existing.len() == branches.len()
            && branches.iter().all(|(after, hold)| {
                existing.iter().any(|(existing, root)| {
                    *existing == after
                        && matches!(
                            self.nodes().get(*root).map(|node| &node.kind),
                            Some(NodeKind::Hold { recipe }) if recipe == hold
                        )
                })
            })
    }
}

fn set_gaps(
    document: &ProjectDocument,
    node: &NodeId,
    gap: Option<&HoldRecipe>,
    branches: &[RepeatGapHold],
    timing: &AudioTimingId,
    context: crate::command::EditContext<'_>,
) -> Result<ProjectDocument, EditError> {
    let (plan, _) = gaps_plan(document, node, gap, branches, context.allocation)?;
    validate_timing(document, timing, context.allocation)?;
    let requested: Vec<_> = branches
        .iter()
        .map(|branch| (branch.after.clone(), branch.hold.clone()))
        .collect();
    if document.repeat_gaps_unchanged(node, gap, &requested) {
        return Ok(document.clone());
    }
    let NodeKind::Repeat { iterations, .. } = &document.nodes()[node].kind else {
        unreachable!()
    };
    let iterations = iterations.clone();
    let mut working = document.clone();
    working.audio_bindings = crate::capture_unbound_audio_bindings(document, timing.clone())?;
    // Later plays and gaps inside the Repeat move with the gaps before them.
    // Like the suffix, each keeps its own pre-edit sample clock: capture every
    // interior entry after the first play once, before the tree changes.
    let first_end = first_play_end(document, node, plan.range.start())?;
    if first_end < plan.range.end() {
        let interior_timing = next_timing(&working, timing)?;
        working = composite::prepare_owners(
            &working,
            &composite::repeat_interior_owners(&working, node)?,
            first_end,
            plan.range.end().0,
            &interior_timing,
        )?;
    }
    if plan.output_duration != plan.range.duration()
        && plan.range.end().0 < document.duration()?.frames()
    {
        let suffix_timing = next_timing(&working, timing)?;
        working = composite::prepare_suffix(
            &working,
            &plan.parent,
            plan.slot + 1,
            plan.range.end(),
            document.duration()?.frames(),
            &suffix_timing,
        )?;
    }
    let mut result = working.clone();
    reduce_gaps(&mut result, node, gap, branches, context.allocation)?;
    // A replaced branch takes its captured clock with it.
    crate::audio_binding_lifecycle::prune(&mut result);
    if let Some(allowances) = context.allowances {
        // Permissions for a default gap end when that gap stops being the
        // default: when it is removed or replaced by an independent Hold.
        allowances.retire_repeat_gaps(node, |after| {
            gap.is_some() && branches.iter().all(|branch| &branch.after != after)
        })?;
        allowances.resize_repeat(node, &iterations)?;
    }
    let command = Command::SetRepeat {
        node: node.clone(),
        plays: iterations.len(),
        gap: gap.cloned(),
    };
    finish(&working, result, &command)
}

fn finish(
    before: &ProjectDocument,
    mut result: ProjectDocument,
    command: &Command,
) -> Result<ProjectDocument, EditError> {
    crate::audio_lineage::reconcile(before, &mut result, command)?;
    result.marks = crate::marks::transform_marks(before, &result, command)?;
    crate::audio_binding_lifecycle::prune(&mut result);
    result.validate()?;
    Ok(result)
}

fn child_range(
    document: &ProjectDocument,
    parent: &NodeId,
    node: &NodeId,
) -> Result<(usize, FrameRange), EditError> {
    document.source_splice_boundary(parent, 0)?;
    let NodeKind::Sequence { children } = &document.nodes()[parent].kind else {
        unreachable!()
    };
    let slot = children
        .iter()
        .position(|child| child == node)
        .ok_or_else(|| unavailable("Repeat target is not a direct child of its Sequence"))?;
    let start = document.source_splice_boundary(parent, slot)?;
    let end = document.source_splice_boundary(parent, slot + 1)?;
    Ok((
        slot,
        FrameRange::new(start, end).map_err(DocumentError::from)?,
    ))
}

fn checked_total(
    document: &ProjectDocument,
    old: FrameDuration,
    new: FrameDuration,
) -> Result<(), EditError> {
    document
        .duration()?
        .frames()
        .checked_sub(old.frames())
        .and_then(|frames| frames.checked_add(new.frames()))
        .ok_or_else(overflow)?;
    Ok(())
}

fn validate_identities(
    document: &ProjectDocument,
    identities: &RepeatSelectionIdentities,
    plan: &RepeatSelectionPlan,
) -> Result<(), EditError> {
    if identities.group.is_some() != plan.needs_group
        || identities.split.nodes.len() != plan.required_split_ids
    {
        return Err(invalid(
            "Repeat identities must match the exact group and Split requirements",
        ));
    }
    let mut used: BTreeSet<_> = document.nodes().keys().collect();
    used.extend(document.audio_lineage().values().map(|value| &value.origin));
    for layout in document.audio_bindings().timings.values() {
        used.extend(layout.nodes().keys());
        used.extend(layout.audio_lineage().values().map(|value| &value.origin));
    }
    for node in std::iter::once(&identities.repeat)
        .chain(identities.group.iter())
        .chain(&identities.split.nodes)
    {
        if !used.insert(node) {
            return Err(EditError::new(
                EditErrorCode::IdentityConflict,
                "Repeat identities must be fresh and distinct",
            ));
        }
    }
    Ok(())
}

fn validate_timing(
    document: &ProjectDocument,
    timing: &AudioTimingId,
    allocation: &RevisionId,
) -> Result<(), EditError> {
    if &timing.allocation != allocation {
        return Err(invalid(
            "Repeat timing allocation must equal the new revision",
        ));
    }
    if document
        .audio_bindings()
        .allocation_ids()
        .contains(allocation)
        || document
            .audio_lineage()
            .values()
            .any(|value| &value.allocation == allocation)
        || document.nodes().values().any(|node| {
            matches!(&node.kind, NodeKind::Repeat { iterations, .. }
            if iterations.segments().any(|(existing, _, _)| existing == allocation))
        })
    {
        return Err(EditError::new(
            EditErrorCode::IdentityConflict,
            "Repeat reuses an existing allocation",
        ));
    }
    Ok(())
}

fn next_timing(
    document: &ProjectDocument,
    base: &AudioTimingId,
) -> Result<AudioTimingId, EditError> {
    // The first ordinal of this allocation that no retained layout uses yet.
    let mut next = base.clone();
    while document.audio_bindings().timings.contains_key(&next) {
        next.ordinal = next
            .ordinal
            .checked_add(1)
            .ok_or_else(|| limit("Repeat requires another timing identity"))?;
    }
    Ok(next)
}

fn invalid(message: &str) -> EditError {
    EditError::new(EditErrorCode::InvalidCommand, message)
}
fn unavailable(message: &str) -> EditError {
    EditError::new(EditErrorCode::SelectionUnavailable, message)
}
fn limit(message: &str) -> EditError {
    EditError::new(EditErrorCode::LimitExceeded, message)
}
fn overflow() -> EditError {
    EditError::new(EditErrorCode::TimingOverflow, "Repeat duration overflow")
}
