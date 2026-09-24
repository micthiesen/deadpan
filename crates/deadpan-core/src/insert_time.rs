//! Atomic pause insertion. Each shifted physical allocation resumes from its
//! exact pre-edit sample entry, including fragments made by an earlier Split.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    AudioClockRoot, AudioLocalPhase, AudioPhaseTerm, AudioPlacementTemplate, AudioReferenceClock,
    AudioResume, AudioTimingId, BeatNode, Command, EditError, EditErrorCode, ExactRatio,
    FrameDuration, HoldRecipe, HoldVideo, InstancePath, MAX_AUDIO_BINDING_ENTRIES,
    MAX_AUDIO_BINDING_TERMS, MAX_DOCUMENT_NODES, NodeId, NodeKind, ProjectDocument, ProjectFrame,
    RetimePurpose, RevisionId, SplitIdentities, Subtree,
};

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
    allocation: &RevisionId,
) -> Result<ProjectDocument, EditError> {
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
    validate_identities(document, id, identities)?;
    let NodeKind::Sequence { children } = &document.nodes()[document.root()].kind else {
        return Err(invalid("pause insertion requires a project-root Sequence"));
    };

    // Resolve once without expanding any repeated plays. The supported suffix
    // is an ordinary physical beat or one transparent partition over it.
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
    let split_nodes = if let Some((target, _)) = &interior {
        let node = &document.nodes()[target];
        let context = match &node.kind {
            NodeKind::Retime {
                child,
                purpose: RetimePurpose::Partition,
                ..
            } if node.framing.is_none() => child,
            _ => target,
        };
        crate::occurrence_edit::subtree_order(document, context)?.len()
            + if context == target { 2 } else { 1 }
    } else {
        0
    };
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
            allocation,
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
                    reference: AudioReferenceClock {
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

    // Split already transported logical mark fragments. Let the ordinary
    // insertion transform shift content-following marks from that intermediate
    // state, while sequence-pinned marks keep their authored project position.
    let insertion = Command::Insert {
        parent: working.root().clone(),
        index: insertion_slot,
        subtree: Subtree {
            root: id.clone(),
            nodes: BTreeMap::from([(id.clone(), BeatNode::hold("Pause", hold.clone()))]),
            overrides: BTreeMap::new(),
        },
    };
    let mut result = working.clone();
    crate::command::reduce(&mut result, &insertion, allocation)?;
    crate::audio_lineage::reconcile(&working, &mut result, &insertion)?;
    result.marks = crate::marks::transform_marks(&working, &result, &insertion)?;
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
fn physical<'a>(
    document: &'a ProjectDocument,
    node: &'a NodeId,
) -> Result<(&'a NodeId, i64), EditError> {
    let mut owner = node;
    let mut start = 0i64;
    let mut partitions = 0usize;
    let mut framed = false;
    loop {
        let node = &document.nodes()[owner];
        framed |= node.framing.is_some();
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
    // A framed Partition is a meaningful retained effect scope. A subsequent
    // Split must keep it, so modern framing can introduce transparent nesting.
    // Old unframed nested inputs keep core17's refusal and frozen replay grammar.
    if partitions > 1 && !framed {
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

fn validate_identities(
    document: &ProjectDocument,
    id: &NodeId,
    identities: &SplitIdentities,
) -> Result<(), EditError> {
    if identities.nodes.len() > MAX_DOCUMENT_NODES {
        return Err(limit(
            "pause Split identity pool exceeds the document node limit",
        ));
    }
    let mut distinct = BTreeSet::from([id]);
    if document.nodes().contains_key(id)
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
