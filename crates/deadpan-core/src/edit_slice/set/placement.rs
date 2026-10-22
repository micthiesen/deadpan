//! A union is one ordinary containing group and one logical mark inventory.

use super::*;

pub(crate) fn apply(
    document: &ProjectDocument,
    command: &Command,
    mut context: crate::command::EditContext<'_>,
) -> Result<ProjectDocument, EditError> {
    let Command::SpliceEditSet {
        parent,
        index,
        set,
        identities,
        timing,
    } = command
    else {
        return Err(invalid("edit set placement requires SpliceEditSet"));
    };
    if &timing.allocation != context.allocation {
        return Err(invalid(
            "edit set timing allocation must equal its new revision",
        ));
    }
    if set.project_id() != document.project_id() {
        return Err(EditError::new(
            EditErrorCode::ProjectConflict,
            "edit set belongs to another project",
        ));
    }
    if set.presentation_basis() != document.presentation_basis() {
        return Err(invalid("edit set basis differs from its destination"));
    }
    document.source_splice_boundary(parent, *index)?;
    let required = set.identity_requirements()?;
    validate_pools(document, set, identities, &required)?;
    document
        .duration()?
        .frames()
        .checked_add(set.duration().frames())
        .ok_or_else(overflow)?;
    if let Some(last) = required.timings.checked_sub(1) {
        let last = u32::try_from(last)
            .ok()
            .and_then(|last| timing.ordinal.checked_add(last))
            .ok_or_else(|| limit("edit set timing ordinal range overflows"))?;
        if document.audio_bindings.timings.keys().any(|known| {
            known.allocation == timing.allocation
                && timing.ordinal <= known.ordinal
                && known.ordinal <= last
        }) {
            return Err(EditError::new(
                EditErrorCode::IdentityConflict,
                "edit set timing range is already retained",
            ));
        }
    }
    // This zero-duration group is private staging for one typed command. No
    // intermediate document, register write or history entry is published.
    let mut working = document.clone();
    working.nodes.insert(
        identities.group.clone(),
        BeatNode::sequence("Copied selection", Vec::new()),
    );
    let NodeKind::Sequence { children } = &mut working
        .nodes
        .get_mut(parent)
        .expect("boundary admitted parent")
        .kind
    else {
        unreachable!()
    };
    children.insert(*index, identities.group.clone());
    working.validate()?;

    let mark_ids = set
        .wire
        .marks
        .keys()
        .cloned()
        .zip(identities.marks.iter().cloned())
        .collect::<BTreeMap<_, _>>();
    let mut collected = BTreeMap::<MarkId, Vec<(MarkFragment, MarkFragment)>>::new();
    let mut timing_offset = 1usize;
    for (member_index, (component, pool)) in set
        .components()
        .iter()
        .zip(&identities.components)
        .enumerate()
    {
        let slots = component
            .identity_requirements()?
            .timings
            .checked_add(usize::from(component.duration() != FrameDuration::ZERO))
            .ok_or_else(|| limit("edit set member timing count overflow"))?;
        let ordinal = if slots == 0 {
            // No record uses this token; do not require an ordinal past the
            // reserved range for an empty final structural component.
            timing.ordinal
        } else {
            u32::try_from(timing_offset)
                .ok()
                .and_then(|offset| timing.ordinal.checked_add(offset))
                .ok_or_else(|| limit("edit set timing ordinal overflow"))?
        };
        let member_marks = component
            .0
            .marks
            .keys()
            .map(|id| mark_ids[id].clone())
            .collect();
        let member = Command::SpliceSlice {
            parent: identities.group.clone(),
            index: member_index,
            slice: component.clone(),
            identities: SlicePasteIdentities {
                authored: OccurrenceIdentities {
                    nodes: pool.nodes.clone(),
                    marks: member_marks,
                },
                aliases: pool.aliases.clone(),
            },
            timing: AudioTimingId {
                allocation: timing.allocation.clone(),
                ordinal,
            },
        };
        let (next, renamed_marks) = super::super::placement::apply_detached_marks(
            &working,
            &member,
            crate::command::EditContext {
                allocation: context.allocation,
                allowances: context.allowances.as_deref_mut(),
                sound_clocks: context.sound_clocks.as_deref_mut(),
            },
        )?;
        for (original_id, original) in &component.0.marks {
            let copied = renamed_marks
                .get(&mark_ids[original_id])
                .ok_or_else(|| invalid("edit set member lost a copied logical mark"))?;
            let old = original.bindings().collect::<Vec<_>>();
            let new = copied.bindings().collect::<Vec<_>>();
            if old.len() != new.len() {
                return Err(invalid(
                    "edit set member changed its physical mark binding count",
                ));
            }
            collected
                .entry(original_id.clone())
                .or_default()
                .extend(old.into_iter().zip(new));
        }
        timing_offset = timing_offset
            .checked_add(slots)
            .ok_or_else(|| limit("edit set timing count overflow"))?;
        working = next;
    }
    let mut merged = BTreeMap::new();
    for (original_id, original) in &set.wire.marks {
        let pairs = collected
            .get(original_id)
            .ok_or_else(|| invalid("edit set omitted a logical mark while placing"))?;
        let bindings = original
            .bindings()
            .map(|binding| {
                pairs
                    .iter()
                    .find(|(old, _)| old == &binding)
                    .map(|(_, copied)| copied.clone())
                    .ok_or_else(|| invalid("edit set cannot map a shared physical mark binding"))
            })
            .collect::<Result<Vec<_>, EditError>>()?;
        let mark = original
            .with_bindings(bindings)
            .ok_or_else(|| invalid("edit set copied an empty logical mark"))?;
        merged.insert(mark_ids[original_id].clone(), mark);
    }
    let finished = crate::marks::finish_slice_marks(&working, merged.clone())?;
    if finished.len() != merged.len()
        || finished
            .iter()
            .any(|(id, mark)| mark.binding_count() != merged[id].binding_count())
    {
        return Err(EditError::new(
            EditErrorCode::SelectionUnavailable,
            "edit set destination cannot retain all owned mark bindings; no copy was published",
        ));
    }
    crate::marks::check_combined_slice_limits(working.marks(), &finished)?;
    working.marks.extend(finished);
    crate::audio_binding_lifecycle::prune(&mut working);
    working.validate()?;
    Ok(working)
}

fn validate_pools(
    document: &ProjectDocument,
    set: &CapturedEditSet,
    identities: &EditSetPasteIdentities,
    required: &EditSetIdentityRequirements,
) -> Result<(), EditError> {
    if identities.components.len() != required.components.len()
        || identities.marks.len() != required.marks
    {
        return Err(invalid(
            "edit set requires its exact component and logical mark identity pools",
        ));
    }
    let mut occupied = document.nodes.keys().cloned().collect::<BTreeSet<_>>();
    for component in set.components() {
        occupied.extend(component.0.nodes.keys().cloned());
        for layout in component.0.audio_bindings.timings.values() {
            occupied.extend(layout.nodes().keys().cloned());
            occupied.extend(
                layout
                    .audio_lineage()
                    .values()
                    .map(|lineage| lineage.origin.clone()),
            );
        }
        occupied.extend(
            component
                .0
                .audio_lineage
                .values()
                .map(|lineage| lineage.origin.clone()),
        );
    }
    for layout in document.audio_bindings.timings.values() {
        occupied.extend(layout.nodes().keys().cloned());
        occupied.extend(
            layout
                .audio_lineage()
                .values()
                .map(|lineage| lineage.origin.clone()),
        );
    }
    occupied.extend(
        document
            .audio_lineage
            .values()
            .map(|lineage| lineage.origin.clone()),
    );
    let mut node_count = 1usize;
    for (pool, required) in identities.components.iter().zip(&required.components) {
        if pool.nodes.len() != required.nodes || pool.aliases.len() != required.aliases {
            return Err(invalid(
                "edit set requires exact member node and alias pools",
            ));
        }
        node_count = node_count
            .checked_add(required.nodes)
            .ok_or_else(|| limit("edit set node count overflow"))?;
    }
    if document
        .nodes
        .len()
        .checked_add(node_count)
        .is_none_or(|count| count > MAX_DOCUMENT_NODES)
    {
        return Err(limit("edit set exceeds the destination node limit"));
    }
    for id in std::iter::once(&identities.group).chain(
        identities
            .components
            .iter()
            .flat_map(|pool| pool.nodes.iter().chain(&pool.aliases)),
    ) {
        if !occupied.insert(id.clone()) {
            return Err(EditError::new(
                EditErrorCode::IdentityConflict,
                "edit set authored and historical identities must be fresh and distinct",
            ));
        }
    }
    let mut occupied_marks = document
        .marks
        .keys()
        .cloned()
        .chain(set.wire.marks.keys().cloned())
        .collect::<BTreeSet<_>>();
    for id in &identities.marks {
        if !occupied_marks.insert(id.clone()) {
            return Err(EditError::new(
                EditErrorCode::IdentityConflict,
                "edit set logical mark identities must be fresh and distinct",
            ));
        }
    }
    crate::marks::check_combined_slice_limits(document.marks(), &set.wire.marks)?;
    Ok(())
}
