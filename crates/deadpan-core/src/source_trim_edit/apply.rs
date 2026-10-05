use super::*;
use crate::command::EditContext;
use crate::{
    AudioEditorialEdges, BeatNode, Command, ExactRatio, FrameDuration, FramingError, HoldAudio,
    HoldRecipe, HoldVideo, MAX_AUDIO_BINDING_ENTRIES, NodeId, NodeKind, PitchPolicy,
    ProjectDocument, SourceTrimOwnerGeometry,
};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn apply(
    document: &ProjectDocument,
    command: &Command,
    mut context: EditContext<'_>,
) -> Result<ProjectDocument, EditError> {
    let Command::ApplySourceTrim {
        parent,
        node,
        right,
        intent,
        resources,
    } = command
    else {
        return Err(invalid("expected a complete Source Trim command"));
    };
    let resolved = document.source_trim_edit(parent, node, right.as_ref(), *intent)?;
    validate_resources(document, &resolved, resources, context.allocation)?;
    if intent.is_zero() {
        return Err(invalid("combined Source Trim resolves to no change"));
    }
    let mut result = capture(document, &resolved, resources)?;
    let boundaries: Vec<_> = resolved.splits.iter().map(|split| split.boundary).collect();
    let (refined, used) = crate::insert_time::sequence_range::split_prepared(
        &result,
        parent,
        &boundaries,
        &resources.split,
        EditContext {
            allocation: context.allocation,
            allowances: context.allowances.as_deref_mut(),
        },
    )?;
    if used != resources.split.nodes.len() {
        return Err(invalid(
            "combined Trim did not consume its exact Split pool",
        ));
    }
    result = refined;
    let baseline = result.clone();
    let a = &resolved.geometry.target;
    let target = install(
        &mut result,
        parent,
        a,
        a.allocation_after,
        resources.target_wrapper.as_ref(),
    )?;
    let mut prefixes = vec![(&a.physical_source, a.physical_prefix)];
    let right_target = if let Some(b) = &resolved.right_after {
        let id = install(
            &mut result,
            parent,
            &b.geometry,
            b.allocation,
            resources.right_wrapper.as_ref(),
        )?;
        prefixes.push((&b.geometry.physical_source, b.geometry.physical_prefix));
        Some(id)
    } else {
        None
    };
    if resolved.footprint.is_some() {
        overlay(
            &baseline,
            &mut result,
            &resolved,
            resources,
            &target,
            right_target.as_ref(),
        )?;
    }
    crate::source_edit::mark_edges(
        &mut result,
        parent,
        &target,
        AudioEditorialEdges {
            start: intent.in_frames != 0 || intent.slip_frames != 0,
            end: intent.out_frames != 0 || intent.roll_frames != 0 || intent.slip_frames != 0,
        },
    )?;
    for filler in &resources.fillers {
        crate::source_edit::mark_edges(
            &mut result,
            parent,
            filler,
            AudioEditorialEdges {
                start: true,
                end: true,
            },
        )?;
    }
    crate::audio_lineage::reconcile(&baseline, &mut result, command)?;
    result.marks =
        crate::marks::transform_marks_with_source_prefixes(&baseline, &result, command, &prefixes)?;
    crate::audio_binding_lifecycle::prune(&mut result);
    if result.structural_durations()?[result.root()] != resolved.geometry.project_duration_after {
        return Err(invalid(
            "combined Trim final extent disagrees with its entry resolution",
        ));
    }
    // Outer apply restores root sound and allowances and performs full validation.
    Ok(result)
}
fn validate_resources(
    document: &ProjectDocument,
    r: &SourceTrimEditResolution,
    ids: &SourceTrimResources,
    allocation: &crate::RevisionId,
) -> Result<(), EditError> {
    if ids.target_wrapper.is_some() != r.required_target_wrapper
        || ids.right_wrapper.is_some() != r.required_right_wrapper
        || ids.split.nodes.len() != r.required_split_nodes
        || ids.fillers.len() != r.required_filler_nodes
        || ids.timing.is_some() != (r.capture != SourceTrimCapture::None)
    {
        return Err(invalid(
            "combined Trim requires exactly its resolved wrapper, Split, filler and timing resources",
        ));
    }
    let mut seen = BTreeSet::new();
    for id in ids
        .target_wrapper
        .iter()
        .chain(ids.right_wrapper.iter())
        .chain(&ids.split.nodes)
        .chain(&ids.fillers)
    {
        if document.nodes().contains_key(id) || !seen.insert(id) {
            return Err(EditError::new(
                EditErrorCode::IdentityConflict,
                "combined Trim identities must be fresh and mutually distinct",
            ));
        }
    }
    if let Some(timing) = &ids.timing
        && (&timing.allocation != allocation
            || document.audio_bindings().timings().contains_key(timing))
    {
        return Err(invalid(
            "combined Trim timing must be fresh and belong to its new revision",
        ));
    }
    Ok(())
}
fn capture(
    document: &ProjectDocument,
    r: &SourceTrimEditResolution,
    ids: &SourceTrimResources,
) -> Result<ProjectDocument, EditError> {
    let mut working = document.clone();
    match r.capture {
        SourceTrimCapture::None => {}
        SourceTrimCapture::Unbound => {
            working.audio_bindings =
                crate::audio_binding_lifecycle::capture_unbound_audio_bindings(
                    document,
                    ids.timing
                        .clone()
                        .ok_or_else(|| invalid("missing Trim capture clock"))?,
                )?;
        }
        SourceTrimCapture::Placements => {
            let timing = ids
                .timing
                .clone()
                .ok_or_else(|| invalid("missing Trim placement clock"))?;
            let captured = crate::audio_binding_lifecycle::capture_for_composite_insertion(
                document,
                timing.clone(),
            )?;
            working.audio_bindings = captured.state;
            // Ripple has no neighbor Splits. Every appended step references this
            // one original layout; no intermediate tree is captured.
            if !r.splits.is_empty() {
                return Err(invalid(
                    "ripple Trim unexpectedly requires retained neighbor Splits",
                ));
            }
            if let Some(layout) = captured.phase_only_layout {
                working.audio_bindings.timings.insert(timing, layout);
            }
            let mut entries = 0usize;
            for (_, _, binding) in working.audio_bindings.owners() {
                for placement in binding.placements() {
                    entries = entries
                        .checked_add(placement.entry_count())
                        .filter(|n| *n <= MAX_AUDIO_BINDING_ENTRIES)
                        .ok_or_else(|| limit("combined Trim binding inventory"))?;
                }
            }
            for group in &r.reanchors {
                let nodes = captured
                    .node_placements
                    .iter()
                    .filter(|(owner, _)| group.owners.contains(*owner))
                    .map(|(owner, p)| (owner.clone(), p.clone()))
                    .collect();
                let gaps = captured
                    .gap_placements
                    .iter()
                    .filter(|(owner, _)| group.owners.contains(*owner))
                    .map(|(owner, p)| (owner.clone(), p.clone()))
                    .collect();
                crate::insert_time::composite::append_anchored_steps(
                    &mut working.audio_bindings.bindings,
                    nodes,
                    &group.owners,
                    group.anchor,
                    group.window,
                    &mut entries,
                )?;
                crate::insert_time::composite::append_anchored_steps(
                    &mut working.audio_bindings.gap_bindings,
                    gaps,
                    &group.owners,
                    group.anchor,
                    group.window,
                    &mut entries,
                )?;
            }
        }
    }
    Ok(working)
}
fn install(
    document: &mut ProjectDocument,
    parent: &NodeId,
    side: &SourceTrimOwnerGeometry,
    allocation: FrameRange,
    wrapper: Option<&NodeId>,
) -> Result<NodeId, EditError> {
    let physical = document
        .nodes
        .get_mut(&side.physical_source)
        .ok_or_else(|| invalid("combined Trim lost its physical Source"))?;
    if side.after.duration != side.before.duration
        && let Some(framing) = &physical.framing
    {
        physical.framing = Some(
            framing
                .prepend_owner_frames(side.physical_prefix, side.before.duration)
                .map_err(|error| {
                    EditError::new(
                        if error == FramingError::Overflow {
                            EditErrorCode::TimingOverflow
                        } else {
                            EditErrorCode::InvalidCommand
                        },
                        error.to_string(),
                    )
                })?,
        );
    }
    if side.physical_prefix != FrameDuration::ZERO {
        physical.audio_treatments = physical
            .audio_treatments
            .with_owner_prefix(side.physical_prefix)
            .map_err(crate::audio_gain::invalid)?;
    }
    if side.physical_prefix != FrameDuration::ZERO {
        // Cutaways and captions stay on the host content they were placed over.
        physical.cutaways =
            crate::cutaways_with_owner_prefix(&physical.cutaways, side.physical_prefix)
                .map_err(crate::DocumentError::from)?;
        physical.captions =
            crate::captions_with_owner_prefix(&physical.captions, side.physical_prefix)
                .map_err(crate::DocumentError::from)?;
    }
    physical.kind = NodeKind::Source {
        source: side.after.clone(),
    };
    if side.physical_prefix != FrameDuration::ZERO
        && let Some(binding) = document
            .audio_bindings
            .bindings
            .get_mut(&side.physical_source)
    {
        *binding = binding.rebase_local(ExactRatio::integer(side.physical_prefix.frames()))?;
    }
    if let Some(wrapper) = wrapper {
        let partition = crate::split::partition(
            &document.nodes()[&side.target].label,
            side.physical_source.clone(),
            allocation.start().0,
            allocation.end().0,
            PitchPolicy::FollowSpeed,
        )?;
        document.nodes.insert(wrapper.clone(), partition);
        let NodeKind::Sequence { children } = &mut document
            .nodes
            .get_mut(parent)
            .ok_or_else(|| invalid("combined Trim lost its parent"))?
            .kind
        else {
            return Err(invalid("combined Trim parent changed kind"));
        };
        let slot = children
            .iter()
            .position(|id| id == &side.target)
            .ok_or_else(|| invalid("combined Trim lost its selected child after refinement"))?;
        children[slot] = wrapper.clone();
        Ok(wrapper.clone())
    } else {
        if side.target != side.physical_source {
            let NodeKind::Retime {
                mapping, duration, ..
            } = &mut document
                .nodes
                .get_mut(&side.target)
                .ok_or_else(|| invalid("combined Trim lost its crop"))?
                .kind
            else {
                return Err(invalid("combined Trim crop changed kind"));
            };
            *mapping = allocation;
            *duration = mapping.duration();
        }
        Ok(side.target.clone())
    }
}
fn overlay(
    baseline: &ProjectDocument,
    result: &mut ProjectDocument,
    r: &SourceTrimEditResolution,
    ids: &SourceTrimResources,
    target: &NodeId,
    right: Option<&NodeId>,
) -> Result<(), EditError> {
    let footprint = r
        .footprint
        .ok_or_else(|| invalid("combined overwrite has no footprint"))?;
    let g = &r.geometry;
    let NodeKind::Sequence { children } = &baseline.nodes()[&g.parent].kind else {
        unreachable!()
    };
    let durations = baseline.structural_durations()?;
    let moves: BTreeMap<_, _> = r.empty_moves.iter().map(|m| (&m.node, m.after)).collect();
    let mut entries = Vec::<(i64, bool, usize, NodeId, i64)>::new();
    let mut removed = Vec::new();
    let mut offset = g.scope_before.start().0;
    for (order, child) in children.iter().enumerate() {
        let next = add(offset, durations[child].frames())?;
        if child == &g.target.target {
            offset = next;
            continue;
        }
        if g.right.as_ref().is_some_and(|b| child == &b.target) {
            if right.is_none() {
                removed.push(child.clone());
            }
            offset = next;
            continue;
        }
        let zero = offset == next;
        if let Some(position) = moves.get(child) {
            entries.push((position.0, false, order, child.clone(), position.0));
        } else if (zero && footprint.start().0 < offset && offset < footprint.end().0)
            || (!zero && footprint.start().0 <= offset && next <= footprint.end().0)
        {
            removed.push(child.clone());
        } else {
            if !zero && offset < footprint.end().0 && footprint.start().0 < next {
                return Err(invalid(
                    "combined overwrite left an unsplit exterior neighbor",
                ));
            }
            entries.push((offset, !zero, order, child.clone(), next));
        }
        offset = next;
    }
    entries.push((
        g.target.output_after.start().0,
        true,
        g.target.slot,
        target.clone(),
        g.target.output_after.end().0,
    ));
    if let (Some(side), Some(id)) = (&r.right_after, right) {
        entries.push((
            side.output.start().0,
            true,
            side.geometry.slot,
            id.clone(),
            side.output.end().0,
        ));
    }
    for (index, (id, interval)) in ids.fillers.iter().zip(&r.fillers).enumerate() {
        result.nodes.insert(
            id.clone(),
            BeatNode::hold(
                "Trim silence",
                HoldRecipe {
                    duration: interval.duration(),
                    video: HoldVideo::Background,
                    audio: HoldAudio::Silence,
                    picture_context: None,
                },
            ),
        );
        entries.push((
            interval.start().0,
            true,
            children.len() + index,
            id.clone(),
            interval.end().0,
        ));
    }
    // Empty sibling runs keep their entry order at ties, before the next positive
    // child. Positive entries must tile the scope exactly with the fresh fillers.
    entries.sort_by_key(|entry| (entry.0, entry.1, entry.2));
    let mut cursor = g.scope_before.start().0;
    let mut final_children = Vec::with_capacity(entries.len());
    for (start, _, _, id, end) in entries {
        if start != cursor {
            return Err(invalid(
                "combined overwrite final children do not exactly tile the scope",
            ));
        }
        cursor = end;
        final_children.push(id);
    }
    if cursor != g.scope_before.end().0 {
        return Err(invalid("combined overwrite changed its scope duration"));
    }
    let NodeKind::Sequence { children } = &mut result
        .nodes
        .get_mut(&g.parent)
        .ok_or_else(|| invalid("combined overwrite lost its parent"))?
        .kind
    else {
        unreachable!()
    };
    *children = final_children;
    for id in removed {
        crate::command::remove_subtree(result, &id)?;
    }
    Ok(())
}
