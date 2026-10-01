//! Relocate one linked interval without observing a deletion-only clock.
//! Whole units retain identity. Split creates only necessary physical contexts.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    AudioTimingId, Command, EditError, EditErrorCode, ExactFrameRange, ExactRatio, FrameDuration,
    FrameRange, MAX_DOCUMENT_NODES, NodeId, NodeKind, PitchPolicy, ProjectDocument, ProjectFrame,
    RetimePurpose, SplitIdentities,
};

/// Both addresses refer to the original document. A seam retains its slot even
/// when empty children share its global boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum MoveRangeDestination {
    Seam {
        parent: NodeId,
        index: usize,
    },
    Interior {
        parent: NodeId,
        target: NodeId,
        at: FrameDuration,
    },
}

impl MoveRangeDestination {
    fn parent(&self) -> &NodeId {
        match self {
            Self::Seam { parent, .. } | Self::Interior { parent, .. } => parent,
        }
    }
}

/// Read-only structural/clock budget. This value supplies no mutation authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SequenceRangeMove {
    pub range: FrameRange,
    pub destination_before: ProjectFrame,
    pub inserted: FrameRange,
    pub removal_join: ProjectFrame,
    /// Joint Split budget, including shared-target refinements only once.
    pub required_ids: usize,
    /// Original lattice capture if needed, then current placement capture.
    pub timing_slots: u32,
    pub is_noop: bool,
}

struct Cuts {
    parent: NodeId,
    index: usize,
    boundaries: BTreeSet<i64>,
}

struct Plan {
    result: SequenceRangeMove,
    cuts: BTreeMap<NodeId, Cuts>,
    capture_before_split: bool,
}

impl ProjectDocument {
    pub fn range_move(
        &self,
        source_parent: &NodeId,
        range: FrameRange,
        destination: &MoveRangeDestination,
    ) -> Result<SequenceRangeMove, EditError> {
        Ok(Plan::new(self, source_parent, range, destination)?.result)
    }
}

impl Plan {
    fn new(
        document: &ProjectDocument,
        source_parent: &NodeId,
        range: FrameRange,
        destination: &MoveRangeDestination,
    ) -> Result<Self, EditError> {
        document.validate()?;
        let selected =
            crate::insert_time::sequence_range::preflight_capture(document, source_parent, range)?;
        let mut cuts = BTreeMap::new();
        for at in [range.start(), range.end()] {
            add_boundary(document, source_parent, at, &mut cuts)?;
        }
        let d = match destination {
            MoveRangeDestination::Seam { parent, index } => {
                document.source_splice_boundary(parent, *index)?
            }
            MoveRangeDestination::Interior { parent, target, at } => {
                let children = children(document, parent)?;
                let index = children
                    .iter()
                    .position(|id| id == target)
                    .ok_or_else(|| invalid("move destination is not a direct child"))?;
                let start = document.source_splice_boundary(parent, index)?;
                let duration = document.node_duration(target)?;
                if *at == FrameDuration::ZERO || *at >= duration {
                    return Err(invalid(
                        "move destination must be strictly inside its child",
                    ));
                }
                crate::insert_time::slice_physical(document, target)?;
                let d = ProjectFrame(start.0.checked_add(at.frames()).ok_or_else(overflow)?);
                add_boundary(document, parent, d, &mut cuts)?;
                d
            }
        };
        if range.start() < d && d < range.end() {
            return Err(invalid("cannot move a range into its removed interior"));
        }
        // Even a boundary inside a selected empty/group subtree would create a
        // cycle. Ordinary Sequence scope validation bounds this ancestor walk.
        let source_children = children(document, source_parent)?;
        let mut owner = destination.parent().clone();
        while &owner != source_parent && &owner != document.root() {
            if source_children[selected.start_index..selected.end_index].contains(&owner) {
                return Err(invalid("move destination belongs to the moved subtree"));
            }
            owner = document
                .parent_of(&owner)
                .ok_or_else(|| invalid("move parent is missing"))?;
        }
        let is_noop = source_parent == destination.parent()
            && (d == range.start() || d == range.end())
            && match destination {
                MoveRangeDestination::Interior { .. } => true,
                MoveRangeDestination::Seam { index, .. } => {
                    *index == selected.start_index || *index == selected.end_index
                }
            };
        let inserted_start = if d >= range.end() {
            d.0.checked_sub(range.duration().frames())
                .ok_or_else(overflow)?
        } else {
            d.0
        };
        let inserted = FrameRange::new(
            ProjectFrame(inserted_start),
            ProjectFrame(
                inserted_start
                    .checked_add(range.duration().frames())
                    .ok_or_else(overflow)?,
            ),
        )
        .map_err(crate::DocumentError::from)?;
        let mut required_ids = 0usize;
        if is_noop {
            cuts.clear();
        } else {
            for (target, cut) in &cuts {
                let first = crate::insert_time::split_node_count(document, target)?;
                let subsequent = first - usize::from(!is_refinement(&document.nodes()[target]));
                let rest = cut.boundaries.len().checked_sub(1).ok_or_else(overflow)?;
                required_ids = subsequent
                    .checked_mul(rest)
                    .and_then(|rest| first.checked_add(rest))
                    .and_then(|count| required_ids.checked_add(count))
                    .ok_or_else(overflow)?;
            }
        }
        if document
            .nodes()
            .len()
            .checked_add(required_ids)
            .is_none_or(|count| count > MAX_DOCUMENT_NODES)
        {
            return Err(limit("move exceeds the temporary document node limit"));
        }
        let capture_before_split =
            required_ids != 0 && crate::audio_binding_lifecycle::has_unbound_recipes(document);
        let changed_time = d != range.start() && d != range.end();
        Ok(Self {
            result: SequenceRangeMove {
                range,
                destination_before: d,
                inserted,
                removal_join: if d < range.start() {
                    range.end()
                } else {
                    range.start()
                },
                required_ids,
                timing_slots: u32::from(capture_before_split) + u32::from(!is_noop && changed_time),
                is_noop,
            },
            cuts,
            capture_before_split,
        })
    }
}

fn add_boundary(
    document: &ProjectDocument,
    parent: &NodeId,
    at: ProjectFrame,
    cuts: &mut BTreeMap<NodeId, Cuts>,
) -> Result<(), EditError> {
    let mut start = document.source_splice_boundary(parent, 0)?.0;
    let durations = document.durations()?;
    for (index, target) in children(document, parent)?.iter().enumerate() {
        let end = start
            .checked_add(durations[target].frames())
            .ok_or_else(overflow)?;
        if start < at.0 && at.0 < end {
            crate::insert_time::slice_physical(document, target)?;
            cuts.entry(target.clone())
                .or_insert_with(|| Cuts {
                    parent: parent.clone(),
                    index,
                    boundaries: BTreeSet::new(),
                })
                .boundaries
                .insert(at.0 - start);
            break;
        }
        start = end;
    }
    Ok(())
}

fn is_refinement(node: &crate::BeatNode) -> bool {
    matches!(
        node.kind,
        NodeKind::Retime {
            purpose: RetimePurpose::Partition,
            ..
        }
    ) && node.framing.is_none()
        && node.audio_treatments.is_empty()
        && node.audio_editorial_edges.is_empty()
}

pub(crate) fn apply(
    document: &ProjectDocument,
    command: &Command,
    mut context: crate::command::EditContext<'_>,
) -> Result<ProjectDocument, EditError> {
    let Command::MoveRange {
        source_revision,
        source_parent,
        range,
        destination,
        identities,
        timing,
    } = command
    else {
        return Err(invalid("range relocation requires MoveRange"));
    };
    if source_revision != document.revision_id() {
        return Err(EditError {
            code: EditErrorCode::RevisionConflict,
            message: "move source revision is stale".into(),
            current_revision: Some(document.revision_id().clone()),
        });
    }
    if &timing.allocation != context.allocation {
        return Err(invalid(
            "move timing allocation must equal the new revision",
        ));
    }
    let plan = Plan::new(document, source_parent, *range, destination)?;
    validate_pools(document, identities, plan.result.required_ids)?;
    if let Some(last) = plan.result.timing_slots.checked_sub(1) {
        let end = timing
            .ordinal
            .checked_add(last)
            .ok_or_else(|| limit("move timing ordinal overflow"))?;
        if document.audio_bindings.timings.keys().any(|id| {
            id.allocation == timing.allocation && id.ordinal >= timing.ordinal && id.ordinal <= end
        }) {
            return Err(EditError::new(
                EditErrorCode::IdentityConflict,
                "move timing range is retained",
            ));
        }
    }
    if plan.result.is_noop {
        return Ok(document.clone());
    }
    let mut working = document.clone();
    if plan.capture_before_split {
        working.audio_bindings = crate::audio_binding_lifecycle::capture_unbound_audio_bindings(
            document,
            timing.clone(),
        )?;
    }
    let mut consumed = 0usize;
    let mut right_at_destination = None;
    for (original, cuts) in &plan.cuts {
        let mut target = original.clone();
        let mut offset = 0;
        for boundary in &cuts.boundaries {
            let count = crate::insert_time::split_node_count(&working, &target)?;
            let end = consumed.checked_add(count).ok_or_else(overflow)?;
            let ids = SplitIdentities {
                nodes: identities
                    .nodes
                    .get(consumed..end)
                    .ok_or_else(|| invalid("move exceeded its joint Split budget"))?
                    .to_vec(),
            };
            let right = ids.nodes[usize::from(!is_refinement(&working.nodes()[&target]))].clone();
            working = crate::split::apply(
                &working,
                &target,
                FrameDuration::new(boundary - offset).map_err(crate::DocumentError::from)?,
                &ids,
                crate::command::EditContext {
                    allocation: context.allocation,
                    allowances: context.allowances.as_deref_mut(),
                },
            )?;
            if matches!(destination, MoveRangeDestination::Interior { target, at, .. }
                if target == original && at.frames() == *boundary)
            {
                right_at_destination = Some(right.clone());
            }
            target = right;
            offset = *boundary;
            consumed = end;
        }
    }
    if consumed != plan.result.required_ids {
        return Err(invalid(
            "move joint Split budget disagrees with its cut plan",
        ));
    }
    let destination_slot = match destination {
        MoveRangeDestination::Seam { parent, index } => plan
            .cuts
            .values()
            .filter(|cut| &cut.parent == parent && cut.index < *index)
            .try_fold(*index, |slot, cut| {
                slot.checked_add(cut.boundaries.len()).ok_or_else(overflow)
            })?,
        MoveRangeDestination::Interior { parent, .. } => children(&working, parent)?
            .iter()
            .position(|id| Some(id) == right_at_destination.as_ref())
            .ok_or_else(|| invalid("move lost its destination Split fence"))?,
    };
    let selected =
        crate::insert_time::sequence_range::selected_children(&working, source_parent, *range)?;
    let d = plan.result.destination_before;
    if d != range.start() && d != range.end() {
        let placement_timing = AudioTimingId {
            allocation: timing.allocation.clone(),
            ordinal: timing
                .ordinal
                .checked_add(u32::from(plan.capture_before_split))
                .ok_or_else(|| limit("move placement timing ordinal overflow"))?,
        };
        working = reanchor(&working, *range, d, &placement_timing)?;
    }
    let mut result = working.clone();
    let count = selected.end - selected.first;
    let slot = if source_parent == destination.parent() {
        if destination_slot >= selected.end {
            destination_slot - count
        } else if destination_slot <= selected.first {
            destination_slot
        } else {
            return Err(invalid(
                "move destination is inside its selected child interval",
            ));
        }
    } else {
        destination_slot
    };
    children_mut(&mut result, source_parent)?.drain(selected.first..selected.end);
    let destination_children = children_mut(&mut result, destination.parent())?;
    if slot > destination_children.len() {
        return Err(invalid(
            "move destination fence exceeds the surviving child list",
        ));
    }
    destination_children.splice(slot..slot, selected.nodes);
    crate::audio_lineage::reconcile(&working, &mut result, command)?;
    result.marks = crate::marks::transform_marks(&working, &result, command)?;
    crate::audio_binding_lifecycle::prune(&mut result);
    result.validate()?;
    Ok(result)
}

fn validate_pools(
    document: &ProjectDocument,
    identities: &SplitIdentities,
    required: usize,
) -> Result<(), EditError> {
    crate::insert_time::validate_identities(document, None, identities)?;
    if identities.nodes.len() < required {
        return Err(invalid("move needs more Split identities"));
    }
    let supplied: BTreeSet<_> = identities.nodes.iter().collect();
    // Never give a new live context the name of a retained historical alias.
    if document.audio_bindings.timings.values().any(|layout| {
        layout.nodes().keys().any(|id| supplied.contains(id))
            || layout
                .audio_lineage()
                .values()
                .any(|lineage| supplied.contains(&lineage.origin))
    }) || document
        .audio_lineage
        .values()
        .any(|lineage| supplied.contains(&lineage.origin))
    {
        return Err(EditError::new(
            EditErrorCode::IdentityConflict,
            "move Split identity is a retained historical alias",
        ));
    }
    Ok(())
}

fn reanchor(
    document: &ProjectDocument,
    moved: FrameRange,
    d: ProjectFrame,
    timing: &AudioTimingId,
) -> Result<ProjectDocument, EditError> {
    let displaced = if d < moved.start() {
        FrameRange::new(d, moved.start())
    } else {
        FrameRange::new(moved.end(), d)
    }
    .map_err(crate::DocumentError::from)?;
    let sets = [
        island_owners(document, moved)?,
        island_owners(document, displaced)?,
    ];
    if !sets[0].is_disjoint(&sets[1]) {
        return Err(invalid("move owner islands overlap"));
    }
    let mut captured =
        crate::audio_binding_lifecycle::capture_for_composite_insertion(document, timing.clone())?;
    let mut working = document.clone();
    working.audio_bindings = captured.state;
    if let Some(layout) = captured.phase_only_layout {
        working
            .audio_bindings
            .timings
            .insert(timing.clone(), layout);
    }
    let mut entries = working
        .audio_bindings
        .owners()
        .flat_map(|(_, _, binding)| binding.placements())
        .try_fold(0usize, |count, placement| {
            count
                .checked_add(placement.entry_count())
                .ok_or_else(overflow)
        })?;
    for (owners, range) in sets.iter().zip([moved, displaced]) {
        let window = ExactFrameRange::new(
            ExactRatio::integer(range.start().0),
            ExactRatio::integer(range.end().0),
        )?;
        let nodes = owners
            .iter()
            .filter_map(|id| {
                captured
                    .node_placements
                    .remove(id)
                    .map(|placement| (id.clone(), placement))
            })
            .collect();
        let gaps = owners
            .iter()
            .filter_map(|id| {
                captured
                    .gap_placements
                    .remove(id)
                    .map(|placement| (id.clone(), placement))
            })
            .collect();
        crate::insert_time::composite::append_steps(
            &mut working.audio_bindings.bindings,
            nodes,
            owners,
            window,
            &mut entries,
        )?;
        crate::insert_time::composite::append_steps(
            &mut working.audio_bindings.gap_bindings,
            gaps,
            owners,
            window,
            &mut entries,
        )?;
    }
    Ok(working)
}

/// Walk ordinary Sequence envelopes until each non-Sequence owned unit lies
/// wholly inside one island. Its hidden context moves with that unit; opaque
/// Preserve inputs remain in their intrinsic clock.
fn island_owners(
    document: &ProjectDocument,
    range: FrameRange,
) -> Result<BTreeSet<NodeId>, EditError> {
    let durations = document.durations()?;
    let mut owners = BTreeSet::new();
    let mut pending = vec![(document.root().clone(), 0i64)];
    while let Some((id, start)) = pending.pop() {
        let end = start
            .checked_add(durations[&id].frames())
            .ok_or_else(overflow)?;
        if end <= range.start().0 || start >= range.end().0 {
            continue;
        }
        if let NodeKind::Sequence { children } = &document.nodes()[&id].kind {
            let mut at = start;
            for child in children {
                pending.push((child.clone(), at));
                at = at
                    .checked_add(durations[child].frames())
                    .ok_or_else(overflow)?;
            }
        } else {
            if start < range.start().0 || end > range.end().0 {
                return Err(invalid("move island cuts an unsplit owned unit"));
            }
            let mut subtree = vec![id];
            while let Some(id) = subtree.pop() {
                if owners.len() >= MAX_DOCUMENT_NODES || !owners.insert(id.clone()) {
                    return Err(limit("move physical-owner traversal"));
                }
                if !matches!(&document.nodes()[&id].kind,
                    NodeKind::Retime { duration, mapping, pitch: PitchPolicy::Preserve, .. }
                        if *duration != mapping.duration())
                {
                    subtree.extend(document.children(&id).cloned());
                }
            }
        }
    }
    Ok(owners)
}

fn children<'a>(document: &'a ProjectDocument, parent: &NodeId) -> Result<&'a [NodeId], EditError> {
    match document.nodes().get(parent).map(|node| &node.kind) {
        Some(NodeKind::Sequence { children }) => Ok(children),
        _ => Err(invalid("move requires an ordinary Sequence parent")),
    }
}
fn children_mut<'a>(
    document: &'a mut ProjectDocument,
    parent: &NodeId,
) -> Result<&'a mut Vec<NodeId>, EditError> {
    match document.nodes.get_mut(parent).map(|node| &mut node.kind) {
        Some(NodeKind::Sequence { children }) => Ok(children),
        _ => Err(invalid("move requires an ordinary Sequence parent")),
    }
}
fn invalid(message: &str) -> EditError {
    EditError::new(EditErrorCode::InvalidCommand, message)
}
fn limit(message: &str) -> EditError {
    EditError::new(EditErrorCode::LimitExceeded, message)
}
fn overflow() -> EditError {
    EditError::new(
        EditErrorCode::TimingOverflow,
        "move range arithmetic overflow",
    )
}
