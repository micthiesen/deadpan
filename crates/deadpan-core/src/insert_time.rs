//! Atomic pause insertion. Each shifted physical allocation resumes from its
//! exact pre-edit sample entry, including fragments made by an earlier Split.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    AudioClockRoot, AudioLocalPhase, AudioPhaseTerm, AudioPlacementTemplate, AudioReanchorStep,
    AudioReferenceClock, AudioResume, AudioTimingId, BeatNode, Command, EditError, EditErrorCode,
    ExactFrameRange, ExactRatio, FrameDuration, HoldRecipe, HoldVideo, InstancePath,
    MAX_AUDIO_BINDING_ENTRIES, MAX_AUDIO_BINDING_TERMS, MAX_DOCUMENT_NODES, NodeId, NodeKind,
    ProjectDocument, ProjectFrame, RetimePurpose, RevisionId, SplitIdentities, Subtree,
};

pub(crate) mod composite;
pub(crate) mod hold_duration;
pub(crate) mod occurrence;
pub(crate) mod retimed;
pub use occurrence::{
    OccurrencePauseIdentities, OccurrencePauseTarget, PauseInsertionTarget, PreparedOccurrenceTime,
    RetimedPauseTarget, prepare_occurrence_time,
};
mod delete;
mod delete_children;
mod delete_range;
pub(crate) mod sequence_range;
mod source_replace;
mod source_splice;
mod target;
pub(crate) use delete::apply as delete;
pub(crate) use delete_children::apply as delete_children;
pub(crate) use delete_range::apply as delete_range;
pub use sequence_range::SequenceRangeEdit;
pub type SourceReplacement = SequenceRangeEdit;
pub(crate) use source_replace::apply as replace_source;
pub(crate) use source_replace::apply_children as replace_source_children;
pub use source_splice::SourceSpliceInterior;
pub(crate) use source_splice::{
    InteriorInsertion as SourceSpliceInsertion, apply as splice_source,
    apply_interior as splice_source_at,
};
pub use target::{InsertTimeSplit, InsertTimeTarget};

struct ShiftedOwner {
    /// Alias in the pre-edit timing layout, independent of any Split copy.
    old: NodeId,
    entry: ExactRatio,
}

/// Insert a pause, also returning the result's structural durations when the
/// insertion computed them (see `command::KnownValidation`).
pub(crate) fn apply_known(
    document: &ProjectDocument,
    at: ProjectFrame,
    hold: &HoldRecipe,
    id: &NodeId,
    identities: &SplitIdentities,
    timing: &AudioTimingId,
    context: crate::command::EditContext<'_>,
) -> Result<(ProjectDocument, Option<crate::command::SharedDurations>), EditError> {
    let allocation = context.allocation;
    if hold.duration == FrameDuration::ZERO {
        return Err(EditError::new(
            EditErrorCode::InvalidDuration,
            "zero-length pause is a no-op; no time or history was changed",
        ));
    }
    if !ordinary_hold(hold) {
        return Err(invalid(
            "inserted pause needs a Background, Freeze or Reverse picture",
        ));
    }
    if &timing.allocation != allocation {
        return Err(invalid(
            "insertion timing allocation must equal the new revision",
        ));
    }
    if !document.derived_clocks().is_empty() {
        // Inside a FollowSpeed speed change the retimed path authors exact
        // clocks; elsewhere the ordinary path runs on sealed recipes.
        validate_identities(document, Some(id), identities)?;
        let extents = document.exact_durations()?;
        if let Some(target) = retimed::locate(document, &|id| extents.get(id).copied(), at)? {
            return retimed::apply(
                document, &target, at, hold, id, identities, timing, allocation,
            )
            .map(|result| (result, None));
        }
    }
    let durations = document.durations()?;
    let total = durations[document.root()].frames();
    if at.0 < 0 || at.0 > total {
        return Err(invalid("pause boundary is outside the project"));
    }
    total
        .checked_add(hold.duration.frames())
        .ok_or_else(overflow)?;
    validate_identities(document, Some(id), identities)?;
    if let Some(target) = retimed::locate(
        document,
        &|id| durations.get(id).map(|d| ExactRatio::integer(d.frames())),
        at,
    )? {
        return retimed::apply(
            document, &target, at, hold, id, identities, timing, allocation,
        )
        .map(|result| (result, None));
    }
    let NodeKind::Sequence { children } = &document.nodes()[document.root()].kind else {
        return Err(invalid("pause insertion requires a project-root Sequence"));
    };

    let legacy = resolve_legacy_suffix(document, children, &durations, at);
    let LegacySuffix {
        slot,
        interior,
        shifted,
    } = match legacy {
        Ok(resolved) => resolved,
        Err(error) if error.code == EditErrorCode::InvalidCommand => {
            let target = target::resolve(document, &durations, at)?;
            if let Some(split) = target.split {
                // The split target must still be a Source, ordinary Hold or
                // admitted transparent fragment. Later siblings may have the
                // complete composite structure supported at a root seam.
                validate_split_budget(document, Some(&split.target), identities)?;
                let placement_timing = AudioTimingId {
                    allocation: timing.allocation.clone(),
                    ordinal: timing
                        .ordinal
                        .checked_add(1)
                        .ok_or_else(|| limit("interior pause needs a second timing identity"))?,
                };
                let mut working = document.clone();
                // Capture sampling lattices BEFORE copying the retained Source
                // or Hold. The new right copy must not acquire a new origin.
                working.audio_bindings =
                    crate::audio_binding_lifecycle::capture_unbound_audio_bindings(
                        document,
                        timing.clone(),
                    )?;
                working = split_captured(document, &working, |working| {
                    crate::split::apply(working, &split.target, split.at, identities, context)
                })?;
                let (parent, slot) = match &target.lift {
                    Some(retime) => {
                        let group =
                            identities
                                .nodes
                                .get(split.required_ids - 1)
                                .ok_or_else(|| {
                                    invalid("pause insertion needs more Split node identities")
                                })?;
                        lift_stage(&mut working, &target.parent, target.index, retime, group)?;
                        (group.clone(), 1)
                    }
                    None => (target.parent.clone(), target.index + 1),
                };
                // The post-Split placement graph has new physical aliases.
                // Give it a distinct immutable identity rather than overwriting
                // the pre-Split graph still referenced by inherited lattices.
                return composite::apply_at(
                    &working,
                    &parent,
                    slot,
                    composite::Insertion {
                        at,
                        total,
                        node: BeatNode::hold("Pause", hold.clone()),
                        id,
                        timing: &placement_timing,
                        allocation,
                    },
                )
                .map(|result| (result, None));
            }
            return composite::apply_at(
                document,
                &target.parent,
                target.index,
                composite::Insertion {
                    at,
                    total,
                    node: BeatNode::hold("Pause", hold.clone()),
                    id,
                    timing,
                    allocation,
                },
            )
            .map(|result| (result, None));
        }
        Err(error) => return Err(error),
    };
    validate_split_budget(
        document,
        interior.as_ref().map(|(target, _)| target),
        identities,
    )?;

    // A current timing table is needed even when every physical node already
    // has a lattice. Resume terms compose the CURRENT mapping, never recapture
    // the old raw recipe or recompute from the Original's frame coordinate.
    //
    // The table may be a provisional slice (see `capture_scoped`). Before the
    // resume terms below, it is read only through the new bindings (whose
    // aliases it projects) and through each reanchored owner's trial step,
    // which names that owner's pre-edit alias. A Split copy keeps its
    // original's binding and aliases.
    // The cut owner's right fragment almost always receives a resume term,
    // so its alias is included up front (`rescope_timing` adds any other).
    let reanchored = shifted
        .iter()
        .enumerate()
        .filter(|(index, shifted)| {
            (*index == 0 && interior.is_some())
                || document
                    .audio_bindings()
                    .bindings()
                    .get(&shifted.old)
                    .is_some_and(|binding| !binding.reanchors.is_empty())
        })
        .map(|(_, shifted)| shifted.old.clone())
        .collect();
    let (bindings, phase_layout) = crate::audio_binding_lifecycle::capture_for_insertion_scoped(
        document,
        timing.clone(),
        reanchored,
    )?;
    let mut working = document.clone();
    working.audio_bindings = bindings;
    // The successful entry query already proved this unchanged borrow. Keep
    // its map alive through the tail, but expose it only to the existing
    // binding-only bridge for the legacy interior Split.
    let entry_durations = if interior.is_some()
        && crate::command_work::local()
        && crate::audio_binding_lifecycle::compact_representation()
        && document.derived_clocks().is_empty()
        && document.retained_durations().is_none()
    {
        Some(std::sync::Arc::new(durations))
    } else {
        None
    };
    // The structural durations of `working`, which binding changes below
    // never alter: the head's, or the Split result's complete validation.
    let working_durations;
    let insertion_slot = if let Some((target, cut)) = &interior {
        let cut = FrameDuration::new(*cut).map_err(crate::DocumentError::from)?;
        let split = || {
            split_captured(document, &working, |working| {
                crate::split::apply_validated(working, target, cut, identities, context)
            })
        };
        let (split, (durations, _)) = match &entry_durations {
            Some(durations) => {
                document.with_proved_durations(std::sync::Arc::clone(durations), || {
                    #[cfg(test)]
                    tests::note_entry_scope(document);
                    split()
                })
            }
            None => split(),
        }?;
        working = split;
        working_durations = Some(std::sync::Arc::new(durations));
        slot + 1
    } else {
        working_durations = document.retained_durations();
        slot
    };

    if let Some(layout) = phase_layout {
        working
            .audio_bindings
            .timings
            .insert(timing.clone(), layout);
    }

    let mut remaining = MAX_AUDIO_BINDING_ENTRIES;
    // One occurrence path, refilled per owner without reallocating.
    let mut instance = InstancePath {
        node: document.root().clone(),
        repeats: Vec::new(),
    };
    for (index, shifted) in shifted.iter().enumerate() {
        let fragment;
        let owner = if index == 0 && interior.is_some() {
            let NodeKind::Sequence { children } = &working.nodes()[working.root()].kind else {
                unreachable!("Split preserves the root Sequence")
            };
            fragment = physical(&working, &children[insertion_slot])?.0.clone();
            &fragment
        } else {
            &shifted.old
        };
        instance.node.clone_from(owner);
        let resolved = working
            .audio_bindings
            .resolve(owner, &instance, remaining)?;
        remaining = remaining
            .checked_sub(resolved.work)
            .ok_or_else(|| limit("pause resume work"))?;
        let anchor = resolved
            .resume
            .as_ref()
            .map_or(resolved.lattice.local_support.start, |resume| {
                resume.local_boundary
            });
        if !working.audio_bindings.bindings[owner].reanchors.is_empty() {
            let binding = &working.audio_bindings.bindings[owner];
            let previous_terms = binding
                .resume
                .as_ref()
                .map_or(0, |resume| resume.phase.terms.len());
            if previous_terms + binding.reanchors.len() >= MAX_AUDIO_BINDING_TERMS {
                return Err(limit(
                    "pause resume exceeds the phase-term and reanchor limit",
                ));
            }
            remaining = remaining
                .checked_sub(1)
                .ok_or_else(|| limit("pause resume work"))?;
            // Reanchor steps are chronological. A later pause must follow
            // them, not alter the legacy initial phase evaluated before them.
            let step = AudioReanchorStep {
                anchor: Default::default(),
                placement: AudioPlacementTemplate {
                    reference_local_offset: crate::ExactRatio::ZERO,
                    gap_after: None,
                    reference: AudioReferenceClock {
                        recipe: crate::AudioRecipeKind::Node,
                        timing: timing.clone(),
                        root: AudioClockRoot::ProjectRootRoundEven,
                        physical: shifted.old.clone(),
                    },
                    arguments: Vec::new(),
                    births: Vec::new(),
                },
                window: Some(ExactFrameRange::new(
                    ExactRatio::integer(at.0),
                    ExactRatio::integer(total),
                )?),
            };
            // An unmoved entry already is the resume anchor; see
            // `reanchor_step_is_inert` for why omitting it is exact.
            if !(crate::audio_binding_lifecycle::compact_representation()
                && working
                    .audio_bindings
                    .reanchor_step_is_inert(owner, binding, &step))
            {
                working
                    .audio_bindings
                    .bindings
                    .get_mut(owner)
                    .expect("captured physical owner")
                    .reanchors
                    .push(step);
            }
            continue;
        }
        let binding = working
            .audio_bindings
            .bindings
            .get_mut(owner)
            .expect("captured physical owner");
        let resume = binding.resume.get_or_insert_with(|| AudioResume {
            local_boundary: anchor,
            phase: AudioLocalPhase::default(),
        });
        if anchor != shifted.entry {
            if resume.phase.terms.len() >= MAX_AUDIO_BINDING_TERMS {
                return Err(limit("pause resume exceeds the phase-term limit"));
            }
            remaining = remaining
                .checked_sub(1)
                .ok_or_else(|| limit("pause resume work"))?;
            resume.phase.terms.push(AudioPhaseTerm {
                placement: AudioPlacementTemplate {
                    reference_local_offset: crate::ExactRatio::ZERO,
                    gap_after: None,
                    reference: AudioReferenceClock {
                        recipe: crate::AudioRecipeKind::Node,
                        timing: timing.clone(),
                        root: AudioClockRoot::ProjectRootRoundEven,
                        physical: shifted.old.clone(),
                    },
                    arguments: Vec::new(),
                    births: Vec::new(),
                },
                from_local: anchor,
                to_local: shifted.entry,
            });
        }
        resume.local_boundary = shifted.entry;
    }
    crate::audio_binding_lifecycle::prune(&mut working);
    // Resume terms name each shifted owner's pre-edit alias in the new table.
    crate::audio_binding_lifecycle::rescope_timing(document, &mut working.audio_bindings, timing)?;
    working.validate_bindings_in_scope(&working.audio_bindings)?;
    let mut structure = None;
    let result = insert_leaf_shared(
        &working,
        working.root(),
        insertion_slot,
        id,
        BeatNode::hold("Pause", hold.clone()),
        allocation,
        working_durations
            .as_deref()
            .filter(|_| crate::command_work::local()),
        &mut structure,
    )?;
    Ok((result, structure))
}

/// Split `working`, a copy of the validated `document` whose only change is
/// a binding state captured and validated against `document`. Complete
/// validation of `working` therefore returns `document`'s durations, so the
/// Split reuses them instead of validating `working` from scratch.
fn split_captured<T>(
    document: &ProjectDocument,
    working: &ProjectDocument,
    split: impl FnOnce(&ProjectDocument) -> Result<T, EditError>,
) -> Result<T, EditError> {
    match document
        .retained_durations()
        .filter(|_| crate::command_work::local())
    {
        Some(durations) => working.with_proved_durations(durations, || split(working)),
        None => split(working),
    }
}

fn validate_split_budget(
    document: &ProjectDocument,
    target: Option<&NodeId>,
    identities: &SplitIdentities,
) -> Result<(), EditError> {
    let split_nodes = target.map_or(Ok(0), |target| split_node_count(document, target))?;
    if document
        .nodes()
        .len()
        .checked_add(split_nodes + 1)
        .is_none_or(|count| count > MAX_DOCUMENT_NODES)
    {
        return Err(limit("pause insertion exceeds the document node limit"));
    }
    if identities.nodes.len() < split_nodes {
        return Err(invalid("pause insertion needs more Split node identities"));
    }

    Ok(())
}

pub(crate) fn split_node_count(
    document: &ProjectDocument,
    target: &NodeId,
) -> Result<usize, EditError> {
    let node = &document.nodes()[target];
    let context = match &node.kind {
        NodeKind::Retime {
            child,
            purpose: RetimePurpose::Partition,
            ..
        } if node.framing.is_none()
            && node.audio_treatments.is_empty()
            && node.audio_editorial_edges.is_empty() =>
        {
            child
        }
        _ => target,
    };
    Ok(
        crate::occurrence_edit::subtree_order(document, context)?.len()
            + if context == target { 2 } else { 1 },
    )
}

struct LegacySuffix {
    slot: usize,
    interior: Option<(NodeId, i64)>,
    shifted: Vec<ShiftedOwner>,
}

struct RootBoundary {
    slot: usize,
    interior: Option<(NodeId, i64)>,
}

fn root_boundary(
    children: &[NodeId],
    durations: &BTreeMap<NodeId, FrameDuration>,
    at: ProjectFrame,
) -> Result<RootBoundary, EditError> {
    let mut offset = 0_i64;
    for (slot, child) in children.iter().enumerate() {
        if offset == at.0 {
            return Ok(RootBoundary {
                slot,
                interior: None,
            });
        }
        let end = offset
            .checked_add(durations[child].frames())
            .ok_or_else(overflow)?;
        if offset < at.0 && at.0 < end {
            return Ok(RootBoundary {
                slot,
                interior: Some((child.clone(), at.0 - offset)),
            });
        }
        offset = end;
    }
    if at.0 == offset {
        Ok(RootBoundary {
            slot: children.len(),
            interior: None,
        })
    } else {
        Err(invalid("pause boundary is outside the project"))
    }
}

fn resolve_legacy_suffix(
    document: &ProjectDocument,
    children: &[NodeId],
    durations: &BTreeMap<NodeId, FrameDuration>,
    at: ProjectFrame,
) -> Result<LegacySuffix, EditError> {
    let mut offset = 0i64;
    let mut slot = children.len();
    let mut interior = None;
    let mut shifted = Vec::new();
    for (index, child) in children.iter().enumerate() {
        let end = offset
            .checked_add(durations[child].frames())
            .ok_or_else(overflow)?;
        if at.0 < end || at.0 <= offset {
            if slot == children.len() {
                slot = index;
                if at.0 > offset {
                    interior = Some((child.clone(), at.0 - offset));
                }
            }
            let (owner, start) = physical(document, child)?;
            let cut = if index == slot {
                (at.0 - offset).max(0)
            } else {
                0
            };
            shifted.push(ShiftedOwner {
                old: owner.clone(),
                entry: ExactRatio::integer(start.checked_add(cut).ok_or_else(overflow)?),
            });
        }
        offset = end;
    }
    Ok(LegacySuffix {
        slot,
        interior,
        shifted,
    })
}

fn insert_leaf_at(
    working: &ProjectDocument,
    parent: &NodeId,
    insertion_slot: usize,
    id: &NodeId,
    node: BeatNode,
    allocation: &RevisionId,
) -> Result<ProjectDocument, EditError> {
    insert_leaf_shared(
        working,
        parent,
        insertion_slot,
        id,
        node,
        allocation,
        None,
        &mut None,
    )
}

/// [`insert_leaf_at`], sharing the result's structural durations between
/// its lineage and mark transforms and with the caller. Given `working`'s
/// structural durations, a root insertion derives the result's from them
/// (`structural_durations_after_root_leaf`) instead of a complete pass.
#[expect(
    clippy::too_many_arguments,
    reason = "one private helper shared by both insertion paths"
)]
fn insert_leaf_shared(
    working: &ProjectDocument,
    parent: &NodeId,
    insertion_slot: usize,
    id: &NodeId,
    node: BeatNode,
    allocation: &RevisionId,
    working_durations: Option<&BTreeMap<NodeId, FrameDuration>>,
    structure: &mut Option<crate::command::SharedDurations>,
) -> Result<ProjectDocument, EditError> {
    // Split already transported logical mark fragments. Let the ordinary
    // insertion transform shift content-following marks from that intermediate
    // state, while sequence-pinned marks keep their authored project position.
    let insertion = Command::Insert {
        parent: parent.clone(),
        index: insertion_slot,
        subtree: Subtree {
            root: id.clone(),
            nodes: BTreeMap::from([(id.clone(), node)]),
            overrides: BTreeMap::new(),
            gap_overrides: BTreeMap::new(),
        },
    };
    let mut result = working.clone();
    crate::command::reduce(&mut result, &insertion, allocation)?;
    if let Some(base) = working_durations
        && structure.is_none()
        && parent == working.root()
    {
        *structure = result
            .structural_durations_after_root_leaf(working, base, id)?
            .map(std::sync::Arc::new);
    }
    crate::audio_lineage::reconcile_shared(working, &mut result, &insertion, structure)?;
    result.marks = crate::marks::transform_marks_shared(working, &result, &insertion, structure)?;
    // The shared command entrypoint locks a provisional presentation basis
    // before full validation. A first Hold is the edit that establishes time.
    Ok(result)
}

fn ordinary_hold(recipe: &HoldRecipe) -> bool {
    matches!(
        recipe.video,
        HoldVideo::Background
            | HoldVideo::Freeze { .. }
            | HoldVideo::Reverse { .. }
            | HoldVideo::Play { .. }
    )
}

/// Return the physical owner and its visible local entry. Transparent
/// partitions retain complete physical children; their selection is allocation,
/// not a new sampling support or a new raw audio recipe.
pub(crate) fn physical<'a>(
    document: &'a ProjectDocument,
    node: &'a NodeId,
) -> Result<(&'a NodeId, i64), EditError> {
    physical_context(document, node, false)
}

/// Capture keeps every intermediate owner intact and therefore admits nested
/// unity windows without requiring the older Split refinement restriction.
pub(crate) fn slice_physical<'a>(
    document: &'a ProjectDocument,
    node: &'a NodeId,
) -> Result<(&'a NodeId, i64), EditError> {
    physical_context(document, node, true)
}

fn physical_context<'a>(
    document: &'a ProjectDocument,
    node: &'a NodeId,
    nested_windows: bool,
) -> Result<(&'a NodeId, i64), EditError> {
    let mut owner = node;
    let mut start = 0i64;
    let mut partitions = 0usize;
    let mut treated = false;
    loop {
        let node = &document.nodes()[owner];
        treated |= node.framing.is_some()
            || !node.audio_treatments.is_empty()
            || !node.audio_editorial_edges.is_empty();
        let NodeKind::Retime {
            child,
            mapping,
            purpose: RetimePurpose::Partition,
            ..
        } = &node.kind
        else {
            break;
        };
        partitions += 1;
        if partitions > crate::MAX_DOCUMENT_DEPTH {
            return Err(limit("pause partition depth"));
        }
        start = start.checked_add(mapping.start().0).ok_or_else(overflow)?;
        owner = child;
    }
    // A treated Partition is a meaningful retained effect scope. A subsequent
    // Split must keep it, so modern treatments can introduce transparent nesting.
    // Old untreated nested inputs keep core17's refusal and frozen replay grammar.
    if partitions > 1 && !treated && !nested_windows {
        return Err(invalid(
            "pause insertion cannot yet shift unframed nested beats",
        ));
    }
    match &document.nodes()[owner].kind {
        NodeKind::Source { .. } => Ok((owner, start)),
        NodeKind::Hold { recipe } if ordinary_hold(recipe) => Ok((owner, start)),
        _ => Err(invalid(
            "pause insertion cannot yet shift nested, repeated, retimed, or generated beats; use a Source or ordinary Hold boundary",
        )),
    }
}

/// A pitch-processing speed change that a pause may split at an output frame.
pub(crate) struct ProcessingStage {
    pub retime: NodeId,
    /// Whether its framing, gain or edges must move to an enclosing group.
    pub lift: bool,
}

/// `node` when it is, or transparently windows, an authored pitch-processing
/// Retime without exact clocks. Each split half keeps the complete processing
/// context, so its output resumes on the retained clock like a Source.
/// A treated stage below a window refuses: lifting its effects would change
/// their clock.
pub(crate) fn processing_stage(
    document: &ProjectDocument,
    node: &NodeId,
) -> Result<Option<ProcessingStage>, EditError> {
    let mut current = node;
    let mut windows = 0usize;
    loop {
        let beat = &document.nodes()[current];
        match &beat.kind {
            NodeKind::Retime {
                child,
                purpose: RetimePurpose::Partition,
                ..
            } if beat.framing.is_none()
                && beat.audio_treatments.is_empty()
                && beat.audio_editorial_edges.is_empty() =>
            {
                windows += 1;
                if windows > crate::MAX_DOCUMENT_DEPTH {
                    return Err(limit("pause partition depth"));
                }
                current = child;
            }
            NodeKind::Retime {
                duration,
                mapping,
                pitch,
                purpose: RetimePurpose::Edit,
                ..
            } if pitch.processes(mapping.duration() == *duration)
                && !document.derived_clocks().contains_key(current) =>
            {
                let lift = beat.framing.is_some()
                    || !beat.audio_treatments.is_empty()
                    || beat.audio_edges != Default::default()
                    || beat.audio_editorial_edges != Default::default();
                if lift && windows > 0 {
                    return Err(invalid(
                        "a pause inside a split speed change with its own framing, gain or edges is not supported yet",
                    ));
                }
                // The pause sits beside the stage's copies, so a group inside
                // it cannot keep animated framing live over the pause.
                let mut pending = vec![current];
                let mut visited = 0usize;
                while let Some(id) = pending.pop() {
                    visited += 1;
                    if visited > crate::MAX_DOCUMENT_NODES {
                        return Err(limit("pause speed change traversal"));
                    }
                    let node = &document.nodes()[id];
                    if id != current
                        && matches!(node.kind, NodeKind::Sequence { .. })
                        && node.framing.as_ref().is_some_and(|framing| {
                            !matches!(framing.value, crate::FramingValue::Static { .. })
                        })
                    {
                        return Err(invalid(
                            "a pause inside a pitch-preserving speed change cannot keep an inner group's moving framing live yet",
                        ));
                    }
                    pending.extend(document.children(id));
                }
                return Ok(Some(ProcessingStage {
                    retime: current.clone(),
                    lift,
                }));
            }
            _ => return Ok(None),
        }
    }
}

/// Move a split speed change's framing, gain and edges to a new group around
/// both halves, so they apply once and stay live over the pause between them.
fn lift_stage(
    document: &mut ProjectDocument,
    parent: &NodeId,
    index: usize,
    retime: &NodeId,
    group: &NodeId,
) -> Result<(), EditError> {
    let NodeKind::Sequence { children } = &document.nodes()[parent].kind else {
        return Err(invalid("pause insertion parent is not a Sequence"));
    };
    let halves = children
        .get(index..index + 2)
        .ok_or_else(|| invalid("split speed change halves are missing"))?
        .to_vec();
    let mut stages = Vec::with_capacity(2);
    for half in &halves {
        let NodeKind::Retime { child, .. } = &document.nodes()[half].kind else {
            return Err(invalid("split speed change half is not a window"));
        };
        stages.push(child.clone());
    }
    if stages.first() != Some(retime) {
        return Err(invalid("split speed change lost its left context"));
    }
    let source = document.nodes()[retime].clone();
    let mut wrapper = BeatNode::sequence(&source.label, halves);
    wrapper.framing = source.framing.clone();
    wrapper.audio_treatments = source.audio_treatments.clone();
    wrapper.audio_edges = source.audio_edges;
    wrapper.audio_editorial_edges = source.audio_editorial_edges;
    for stage in &stages {
        let node = document
            .nodes
            .get_mut(stage)
            .ok_or_else(|| invalid("split speed change context is missing"))?;
        node.framing = None;
        node.audio_treatments = Default::default();
        node.audio_edges = Default::default();
        node.audio_editorial_edges = Default::default();
    }
    document.nodes.insert(group.clone(), wrapper);
    let NodeKind::Sequence { children } =
        &mut document.nodes.get_mut(parent).expect("checked parent").kind
    else {
        unreachable!("checked Sequence")
    };
    children.splice(index..index + 2, [group.clone()]);
    Ok(())
}

pub(crate) fn validate_identities(
    document: &ProjectDocument,
    id: Option<&NodeId>,
    identities: &SplitIdentities,
) -> Result<(), EditError> {
    if identities.nodes.len() > MAX_DOCUMENT_NODES {
        return Err(limit(
            "pause Split identity pool exceeds the document node limit",
        ));
    }
    let mut distinct = BTreeSet::from_iter(id);
    if id.is_some_and(|id| document.nodes().contains_key(id))
        || identities
            .nodes
            .iter()
            .any(|next| document.nodes().contains_key(next) || !distinct.insert(next))
    {
        return Err(EditError::new(
            EditErrorCode::IdentityConflict,
            "pause and Split identities must be fresh and distinct",
        ));
    }
    Ok(())
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
        "pause insertion time overflow",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;
    use std::cell::Cell;
    use std::sync::Arc;

    thread_local! {
        static ENTRY_SCOPES: Cell<usize> = const { Cell::new(0) };
    }

    pub(super) fn note_entry_scope(document: &ProjectDocument) {
        assert!(document.retained_durations().is_some());
        ENTRY_SCOPES.with(|count| count.set(count.get().checked_add(1).unwrap()));
    }

    fn scopes() -> usize {
        ENTRY_SCOPES.with(Cell::get)
    }

    fn id(name: &str) -> NodeId {
        NodeId::new(name).unwrap()
    }

    fn hold(frames: i64) -> HoldRecipe {
        HoldRecipe {
            duration: FrameDuration::new(frames).unwrap(),
            picture_context: None,
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        }
    }

    fn fixture(count: usize) -> ProjectDocument {
        let mut document = ProjectDocument::new(
            ProjectId::new("split-proof").unwrap(),
            RevisionId::new("entry").unwrap(),
            crate::basis::default_basis(),
            id("root"),
        )
        .unwrap();
        let children: Vec<_> = (0..count).map(|i| id(&format!("b{i:04}"))).collect();
        for child in &children {
            document
                .nodes
                .insert(child.clone(), BeatNode::hold("Beat", hold(8)));
        }
        document
            .nodes
            .insert(id("root"), BeatNode::sequence("Root", children));
        document.validate().unwrap();
        document
    }

    fn pause(document: &ProjectDocument, revision: &str, at: i64) -> CommandRequest {
        let split = document
            .insert_time_target(ProjectFrame(at))
            .unwrap()
            .split
            .unwrap();
        CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: RevisionId::new(revision).unwrap(),
            command: Command::InsertTime {
                at: ProjectFrame(at),
                hold: hold(3),
                id: id(&format!("{revision}-pause")),
                identities: SplitIdentities {
                    nodes: (0..split.required_ids)
                        .map(|i| id(&format!("{revision}-split-{i}")))
                        .collect(),
                },
                timing: AudioTimingId {
                    allocation: RevisionId::new(revision).unwrap(),
                    ordinal: 0,
                },
            },
        }
    }

    fn compare_success(before: &ProjectDocument, request: &CommandRequest) -> ProjectDocument {
        assert!(before.retained_durations().is_none());
        let bytes = before.to_compact_json().unwrap();
        let count = scopes();
        let (edit, after) = crate::apply_with_result(before, request).unwrap();
        assert_eq!(scopes() - count, 1);
        assert!(before.retained_durations().is_none());
        assert!(after.retained_durations().is_none());
        let reference =
            crate::with_reference_command_work(|| crate::apply_with_result(before, request))
                .unwrap();
        assert_eq!(scopes() - count, 1);
        let validated = ValidatedDocument::new(Arc::new(before.clone())).unwrap();
        let scoped = validated
            .scope(|| crate::apply_with_result(validated.document(), request))
            .unwrap();
        assert_eq!(scopes() - count, 1);
        assert!(validated.retained_durations().is_none());
        assert_eq!((&edit, &after), (&reference.0, &reference.1));
        assert_eq!((&edit, &after), (&scoped.0, &scoped.1));
        assert_eq!(
            after.to_compact_json().unwrap(),
            reference.1.to_compact_json().unwrap()
        );
        assert_eq!(
            serde_json::to_string(&edit).unwrap(),
            serde_json::to_string(&reference.0).unwrap()
        );
        assert_eq!(edit.forward.apply(before).unwrap(), after);
        assert_eq!(edit.inverse.apply(&after).unwrap(), *before);
        assert_eq!(edit.forward.restores(before, &after), Ok(true));
        assert_eq!(edit.inverse.restores(&after, before), Ok(true));
        assert_eq!(before.to_compact_json().unwrap(), bytes);
        after.validate().unwrap();
        after
    }

    #[test]
    fn insert_split_proof_plain_and_scoped_successive_pauses_match_complete_reference() {
        let mut document = fixture(128);
        document.marks.insert(
            MarkId::new("cue").unwrap(),
            Mark {
                owner: id("b0004"),
                label: "Cue".into(),
                boundary: BoundaryAnchor {
                    coordinate: Anchor::Local {
                        node: id("b0004"),
                        position: ExactRatio::integer(3),
                    },
                    bias: InsertionBias::Right,
                },
                loss_policy: AnchorLossPolicy::DeleteOwned,
                state: MarkState::Bound,
                fragments: Vec::new(),
            },
        );
        for (index, at) in [67, 151, 243].into_iter().enumerate() {
            let request = pause(&document, &format!("r{index}"), at);
            document = compare_success(&document, &request);
        }
        assert!(!document.audio_bindings().is_empty());
        assert!(!document.marks().is_empty());
    }

    #[test]
    fn insert_split_proof_reference_representation_and_seams_keep_existing_path() {
        let document = fixture(16);
        let request = pause(&document, "pause", 19);
        let count = scopes();
        let local = crate::with_reference_timing_representation(|| {
            crate::apply_with_result(&document, &request)
        })
        .unwrap();
        let reference = crate::with_reference_timing_representation(|| {
            crate::with_reference_command_work(|| crate::apply_with_result(&document, &request))
        })
        .unwrap();
        assert_eq!(local, reference);
        assert_eq!(scopes(), count);
        let mut seam = request.clone();
        let Command::InsertTime { at, identities, .. } = &mut seam.command else {
            unreachable!()
        };
        *at = ProjectFrame(16);
        identities.nodes.clear();
        assert_eq!(
            crate::apply_with_result(&document, &seam).unwrap(),
            crate::with_reference_command_work(|| crate::apply_with_result(&document, &seam))
                .unwrap()
        );
        assert_eq!(scopes(), count);
        assert!(document.retained_durations().is_none());
    }

    fn compare_error(document: &ProjectDocument, request: &CommandRequest) -> EditError {
        let before = document.to_compact_json().unwrap();
        let error = crate::apply_with_result(document, request).unwrap_err();
        let reference =
            crate::with_reference_command_work(|| crate::apply_with_result(document, request))
                .unwrap_err();
        assert_eq!(error, reference);
        assert_eq!(document.to_compact_json().unwrap(), before);
        assert!(document.retained_durations().is_none());
        error
    }

    #[test]
    fn insert_split_proof_changed_same_revision_inputs_fail_before_installing_scope() {
        let document = fixture(16);
        let request = pause(&document, "pause", 19);
        let mut label = document.clone();
        label.nodes.get_mut(&id("b0000")).unwrap().label.push('\0');
        let mut missing = document.clone();
        missing.nodes.remove(&id("b0000"));
        let mut bindings = document.clone();
        bindings.audio_bindings = crate::audio_binding_lifecycle::capture_unbound_audio_bindings(
            &document,
            AudioTimingId {
                allocation: RevisionId::new("captured").unwrap(),
                ordinal: 0,
            },
        )
        .unwrap();
        bindings
            .audio_bindings
            .bindings
            .get_mut(&id("b0000"))
            .unwrap()
            .lattice
            .reference
            .physical = id("absent");
        let count = scopes();
        for changed in [label, missing, bindings] {
            assert_eq!(changed.revision_id(), document.revision_id());
            compare_error(&changed, &request);
            assert_eq!(scopes(), count);
        }
        compare_success(&document, &request);
    }

    #[test]
    fn insert_split_proof_mark_growth_refusal_cleans_both_scopes() {
        let mut document = fixture(1);
        let target = id("b0000");
        document.marks.insert(
            MarkId::new("cue").unwrap(),
            Mark {
                owner: target.clone(),
                label: "Cue".into(),
                boundary: BoundaryAnchor {
                    coordinate: Anchor::Local {
                        node: target.clone(),
                        position: ExactRatio::ZERO,
                    },
                    bias: InsertionBias::Right,
                },
                loss_policy: AnchorLossPolicy::DeleteOwned,
                state: MarkState::Bound,
                fragments: (1..MAX_MARK_BINDINGS)
                    .map(|i| MarkFragment {
                        owner: target.clone(),
                        coordinate: Anchor::Local {
                            node: target.clone(),
                            position: ExactRatio::new(i as i128, MAX_MARK_BINDINGS as i128)
                                .unwrap(),
                        },
                        state: MarkState::Bound,
                    })
                    .collect(),
            },
        );
        document.validate().unwrap();
        let request = pause(&document, "pause", 3);
        let count = scopes();
        assert_eq!(
            compare_error(&document, &request).code,
            EditErrorCode::LimitExceeded
        );
        assert_eq!(scopes() - count, 1);
        let fresh = fixture(1);
        compare_success(&fresh, &pause(&fresh, "recovery", 3));
    }

    #[test]
    fn insert_split_proof_phase_budget_refusal_and_early_identity_limits_are_exact() {
        let mut document = fixture(1);
        let mut state = crate::audio_binding_lifecycle::capture_unbound_audio_bindings(
            &document,
            AudioTimingId {
                allocation: RevisionId::new("captured").unwrap(),
                ordinal: 0,
            },
        )
        .unwrap();
        let binding = state.bindings.get_mut(&id("b0000")).unwrap();
        binding.resume = Some(AudioResume {
            local_boundary: ExactRatio::ZERO,
            phase: AudioLocalPhase {
                constant: ExactRatio::ZERO,
                terms: vec![
                    AudioPhaseTerm {
                        placement: binding.lattice.clone(),
                        from_local: ExactRatio::ZERO,
                        to_local: ExactRatio::ZERO
                    };
                    MAX_AUDIO_BINDING_TERMS
                ],
            },
        });
        document.audio_bindings = state;
        document.validate().unwrap();
        let request = pause(&document, "pause", 3);
        assert_eq!(
            compare_error(&document, &request).code,
            EditErrorCode::LimitExceeded
        );
        let fresh = fixture(1);
        let mut request = pause(&fresh, "early", 3);
        let Command::InsertTime { identities, .. } = &mut request.command else {
            unreachable!()
        };
        identities.nodes = vec![id("unused"); MAX_DOCUMENT_NODES + 1];
        let count = scopes();
        assert_eq!(
            compare_error(&fresh, &request).code,
            EditErrorCode::LimitExceeded
        );
        assert_eq!(scopes(), count);
        compare_success(&fresh, &pause(&fresh, "recovery", 3));
    }

    #[test]
    fn insert_split_proof_binding_only_bridge_is_exact_and_pops_on_error_and_unwind() {
        let document = fixture(16);
        let durations = Arc::new(document.durations().unwrap());
        let mut working = document.clone();
        working.audio_bindings = crate::audio_binding_lifecycle::capture_unbound_audio_bindings(
            &document,
            AudioTimingId {
                allocation: RevisionId::new("captured").unwrap(),
                ordinal: 0,
            },
        )
        .unwrap();
        assert_eq!(working.durations().unwrap(), *durations);
        let sentinel = super::invalid("test-only Split refusal");
        let error = document
            .with_proved_durations(Arc::clone(&durations), || {
                split_captured(&document, &working, |borrow| -> Result<(), EditError> {
                    assert!(Arc::ptr_eq(
                        &borrow.retained_durations().unwrap(),
                        &durations
                    ));
                    Err(sentinel.clone())
                })
            })
            .unwrap_err();
        assert_eq!(error, sentinel);
        assert!(document.retained_durations().is_none());
        assert!(working.retained_durations().is_none());
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            document.with_proved_durations(Arc::clone(&durations), || {
                split_captured(&document, &working, |borrow| -> Result<(), EditError> {
                    assert!(Arc::ptr_eq(
                        &borrow.retained_durations().unwrap(),
                        &durations
                    ));
                    panic!("test-only Split unwind");
                })
            })
        }));
        assert!(panic.is_err());
        assert!(document.retained_durations().is_none());
        assert!(working.retained_durations().is_none());
        compare_success(&document, &pause(&document, "recovery", 19));
    }
}
