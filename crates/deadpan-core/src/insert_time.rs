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

pub(crate) fn apply(
    document: &ProjectDocument,
    at: ProjectFrame,
    hold: &HoldRecipe,
    id: &NodeId,
    identities: &SplitIdentities,
    timing: &AudioTimingId,
    context: crate::command::EditContext<'_>,
) -> Result<ProjectDocument, EditError> {
    let allocation = context.allocation;
    if hold.duration == FrameDuration::ZERO {
        return Err(EditError::new(
            EditErrorCode::InvalidDuration,
            "zero-length pause is a no-op; no time or history was changed",
        ));
    }
    if !ordinary_hold(hold) {
        return Err(invalid(
            "inserted pause needs a Background or Freeze fallback",
        ));
    }
    if &timing.allocation != allocation {
        return Err(invalid(
            "insertion timing allocation must equal the new revision",
        ));
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
                working =
                    crate::split::apply(&working, &split.target, split.at, identities, context)?;
                // The post-Split placement graph has new physical aliases.
                // Give it a distinct immutable identity rather than overwriting
                // the pre-Split graph still referenced by inherited lattices.
                return composite::apply_at(
                    &working,
                    &target.parent,
                    target.index + 1,
                    composite::Insertion {
                        at,
                        total,
                        node: BeatNode::hold("Pause", hold.clone()),
                        id,
                        timing: &placement_timing,
                        allocation,
                    },
                );
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
            );
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
    let (bindings, phase_layout) =
        crate::audio_binding_lifecycle::capture_for_insertion(document, timing.clone())?;
    let mut working = document.clone();
    working.audio_bindings = bindings;
    let insertion_slot = if let Some((target, cut)) = &interior {
        working = crate::split::apply(
            &working,
            target,
            FrameDuration::new(*cut).map_err(crate::DocumentError::from)?,
            identities,
            context,
        )?;
        slot + 1
    } else {
        slot
    };

    if let Some(layout) = phase_layout {
        working
            .audio_bindings
            .timings
            .insert(timing.clone(), layout);
    }

    let mut remaining = MAX_AUDIO_BINDING_ENTRIES;
    for (index, shifted) in shifted.iter().enumerate() {
        let owner = if index == 0 && interior.is_some() {
            let NodeKind::Sequence { children } = &working.nodes()[working.root()].kind else {
                unreachable!("Split preserves the root Sequence")
            };
            physical(&working, &children[insertion_slot])?.0.clone()
        } else {
            shifted.old.clone()
        };
        let resolved = working.audio_bindings.resolve(
            &owner,
            &InstancePath {
                node: owner.clone(),
                repeats: Vec::new(),
            },
            remaining,
        )?;
        remaining = remaining
            .checked_sub(resolved.work)
            .ok_or_else(|| limit("pause resume work"))?;
        let anchor = resolved
            .resume
            .as_ref()
            .map_or(resolved.lattice.local_support.start, |resume| {
                resume.local_boundary
            });
        let binding = working
            .audio_bindings
            .bindings
            .get_mut(&owner)
            .expect("captured physical owner");
        if !binding.reanchors.is_empty() {
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
            binding.reanchors.push(AudioReanchorStep {
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
            });
            continue;
        }
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
    working.audio_bindings.validate_for(&working)?;
    insert_pause(&working, insertion_slot, id, hold, allocation)
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

fn insert_pause(
    working: &ProjectDocument,
    insertion_slot: usize,
    id: &NodeId,
    hold: &HoldRecipe,
    allocation: &RevisionId,
) -> Result<ProjectDocument, EditError> {
    insert_leaf_at(
        working,
        working.root(),
        insertion_slot,
        id,
        BeatNode::hold("Pause", hold.clone()),
        allocation,
    )
}

fn insert_leaf_at(
    working: &ProjectDocument,
    parent: &NodeId,
    insertion_slot: usize,
    id: &NodeId,
    node: BeatNode,
    allocation: &RevisionId,
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
    crate::audio_lineage::reconcile(working, &mut result, &insertion)?;
    result.marks = crate::marks::transform_marks(working, &result, &insertion)?;
    // The shared command entrypoint locks a provisional presentation basis
    // before full validation. A first Hold is the edit that establishes time.
    Ok(result)
}

fn ordinary_hold(recipe: &HoldRecipe) -> bool {
    matches!(
        recipe.video,
        HoldVideo::Background | HoldVideo::Freeze { .. }
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
