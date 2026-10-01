//! Atomic placement of a closed slice. Destination splits retain their original
//! clocks; replacement captures the suffix before removing selected contents.

use super::*;

enum Destination<'a> {
    Seam {
        index: usize,
        boundary: ProjectFrame,
    },
    Interior {
        target: &'a NodeId,
        at: FrameDuration,
        resolved: SourceSpliceInterior,
    },
    Replacement {
        range: FrameRange,
        resolved: SequenceRangeEdit,
    },
}

struct Placement<'a> {
    parent: &'a NodeId,
    slice: &'a CapturedEditSlice,
    identities: &'a SlicePasteIdentities,
    splits: Option<&'a SplitIdentities>,
    timing: &'a AudioTimingId,
    destination: Destination<'a>,
}

impl<'a> Placement<'a> {
    fn resolve(document: &ProjectDocument, command: &'a Command) -> Result<Self, EditError> {
        let (parent, slice, identities, timing, splits, destination) = match command {
            Command::SpliceSlice {
                parent,
                index,
                slice,
                identities,
                timing,
            } => (
                parent,
                slice,
                identities,
                timing,
                None,
                Destination::Seam {
                    index: *index,
                    boundary: document.source_splice_boundary(parent, *index)?,
                },
            ),
            Command::SpliceSliceAt {
                parent,
                target,
                at,
                slice,
                identities,
                split_identities,
                timing,
            } => (
                parent,
                slice,
                identities,
                timing,
                Some(split_identities),
                Destination::Interior {
                    target,
                    at: *at,
                    resolved: document.slice_splice_interior(parent, target, *at, slice)?,
                },
            ),
            Command::ReplaceSlice {
                parent,
                range,
                slice,
                identities,
                split_identities,
                timing,
            } => (
                parent,
                slice,
                identities,
                timing,
                Some(split_identities),
                Destination::Replacement {
                    range: *range,
                    resolved: document.slice_replacement(parent, *range, slice)?,
                },
            ),
            _ => return Err(invalid("slice placement requires a slice command")),
        };
        slice.check_destination(document)?;
        Ok(Self {
            parent,
            slice,
            identities,
            splits,
            timing,
            destination,
        })
    }

    fn split_count(&self) -> usize {
        match &self.destination {
            Destination::Seam { .. } => 0,
            Destination::Interior { resolved, .. } => resolved.required_ids,
            Destination::Replacement { resolved, .. } => resolved.required_ids,
        }
    }
}

struct Timings {
    split: Option<AudioTimingId>,
    suffix: Option<AudioTimingId>,
    imported: AudioTimingId,
}

impl Timings {
    fn new(
        document: &ProjectDocument,
        placement: &Placement<'_>,
        imported: usize,
        total: i64,
    ) -> Result<Self, EditError> {
        let splits = placement.split_count() != 0;
        // Seam paste retains its original reserved suffix slot, even at the end.
        let suffix = placement.slice.duration() != FrameDuration::ZERO
            && match placement.destination {
                Destination::Replacement { range, .. } => range.end().0 < total,
                _ => true,
            };
        let destination = u32::from(splits) + u32::from(suffix);
        let count = u32::try_from(imported)
            .ok()
            .and_then(|count| count.checked_add(destination))
            .ok_or_else(|| limit("slice timing count overflows"))?;
        let timing = |offset: u32| -> Result<AudioTimingId, EditError> {
            Ok(AudioTimingId {
                allocation: placement.timing.allocation.clone(),
                ordinal: placement
                    .timing
                    .ordinal
                    .checked_add(offset)
                    .ok_or_else(|| limit("slice timing ordinal range overflows"))?,
            })
        };
        if let Some(last) = count.checked_sub(1) {
            let last = timing(last)?;
            if document.audio_bindings.timings.keys().any(|id| {
                id.allocation == last.allocation
                    && id.ordinal >= placement.timing.ordinal
                    && id.ordinal <= last.ordinal
            }) {
                return Err(EditError::new(
                    EditErrorCode::IdentityConflict,
                    "slice timing range is already retained",
                ));
            }
        }
        Ok(Self {
            split: splits.then(|| timing(0)).transpose()?,
            suffix: suffix.then(|| timing(u32::from(splits))).transpose()?,
            // No imported records means the value is unused; do not require an
            // additional ordinal beyond the actually consumed destination slots.
            imported: timing(if imported == 0 { 0 } else { destination })?,
        })
    }
}

pub(crate) fn apply(
    document: &ProjectDocument,
    command: &Command,
    context: crate::command::EditContext<'_>,
) -> Result<ProjectDocument, EditError> {
    let placement = Placement::resolve(document, command)?;
    if &placement.timing.allocation != context.allocation {
        return Err(invalid(
            "slice timing allocation must equal the new revision",
        ));
    }
    let total = document.duration()?.frames();
    let removed_duration = match placement.destination {
        Destination::Replacement { range, .. } => range.duration().frames(),
        _ => 0,
    };
    (total - removed_duration)
        .checked_add(placement.slice.duration().frames())
        .ok_or_else(overflow)?;
    let requirements = placement.slice.identity_requirements()?;
    rename::validate_pools(
        document,
        &placement.slice.0,
        placement.identities,
        requirements,
        placement.splits.map_or(&[], |ids| ids.nodes.as_slice()),
        placement.split_count(),
    )?;
    let timings = Timings::new(document, &placement, requirements.timings, total)?;
    check_combined(document, placement.slice)?;
    let imported = rename::prepare(&placement.slice.0, placement.identities, &timings.imported)?;

    let mut working = match (&placement.destination, &timings.split) {
        (Destination::Interior { target, at, .. }, Some(timing)) => {
            let mut working = document.clone();
            working.audio_bindings =
                crate::audio_binding_lifecycle::capture_unbound_audio_bindings(
                    document,
                    timing.clone(),
                )?;
            crate::split::apply(
                &working,
                target,
                *at,
                placement.splits.expect("interior pools"),
                context,
            )?
        }
        (Destination::Replacement { range, .. }, Some(timing)) => {
            crate::insert_time::sequence_range::split_endpoints(
                document,
                placement.parent,
                *range,
                placement.splits.expect("replacement pools"),
                timing,
                context,
            )?
        }
        _ => document.clone(),
    };
    // Split can duplicate retained mark fragments and owner treatments. Charge
    // those actual bounded results together with the import before installation.
    check_combined(&working, placement.slice)?;
    let (first, end, removed, suffix_boundary) = match &placement.destination {
        Destination::Seam { index, boundary } => (*index, *index, Vec::new(), *boundary),
        Destination::Interior { resolved, .. } => (
            resolved.index + 1,
            resolved.index + 1,
            Vec::new(),
            resolved.boundary,
        ),
        Destination::Replacement { range, .. } => {
            let selected = crate::insert_time::sequence_range::selected_children(
                &working,
                placement.parent,
                *range,
            )?;
            (selected.first, selected.end, selected.nodes, range.end())
        }
    };
    if let Some(timing) = &timings.suffix {
        working = crate::insert_time::composite::prepare_suffix(
            &working,
            placement.parent,
            end,
            suffix_boundary,
            total,
            timing,
        )?;
    }
    // All clocks above refer to the unchanged-duration split tree. Replace its
    // child interval directly; no shorter deletion-only clock is ever observed.
    let mut result = working.clone();
    imported.install(&mut result)?;
    let NodeKind::Sequence { children } = &mut result
        .nodes
        .get_mut(placement.parent)
        .expect("admitted parent")
        .kind
    else {
        unreachable!()
    };
    children.splice(first..end, [placement.identities.authored.nodes[0].clone()]);
    for child in removed {
        crate::command::remove_subtree(&mut result, &child)?;
    }
    let retained_lineage: BTreeMap<_, _> = result
        .audio_lineage
        .iter()
        .filter(|(id, _)| !working.nodes.contains_key(*id))
        .map(|(id, lineage)| (id.clone(), lineage.clone()))
        .collect();
    if placement.slice.duration() != FrameDuration::ZERO {
        crate::audio_lineage::reconcile(&working, &mut result, command)?;
    }
    result.audio_lineage.extend(retained_lineage);
    let mut marks = std::mem::take(&mut result.marks);
    marks.retain(|id, _| !working.marks.contains_key(id));
    result.marks = if placement.slice.duration() == FrameDuration::ZERO {
        working.marks.clone()
    } else {
        crate::marks::transform_marks(&working, &result, command)?
    };
    let copied = crate::marks::finish_slice_marks(&result, marks)?;
    result.marks.extend(copied);
    crate::audio_binding_lifecycle::prune(&mut result);
    result.validate()?;
    Ok(result)
}

fn check_combined(document: &ProjectDocument, slice: &CapturedEditSlice) -> Result<(), EditError> {
    for (id, asset) in &slice.0.assets {
        if document
            .assets
            .get(id)
            .is_some_and(|existing| existing != asset)
        {
            return Err(EditError::new(
                EditErrorCode::ImmutableAsset,
                "captured asset differs from the immutable destination asset",
            ));
        }
    }
    if document
        .assets
        .len()
        .checked_add(
            slice
                .0
                .assets
                .keys()
                .filter(|id| !document.assets.contains_key(*id))
                .count(),
        )
        .is_none_or(|count| count > MAX_DOCUMENT_ASSETS)
    {
        return Err(limit("slice exceeds the destination asset limit"));
    }
    crate::audio_gain::validate_nodes_with_limit(
        document.nodes.values().chain(slice.0.nodes.values()),
        crate::audio_gain::MAX_ISOLATED_GAIN_RECORDS,
    )?;
    crate::picture_context::validate_nodes_with_limit(
        document.nodes.values().chain(slice.0.nodes.values()),
        crate::picture_context::MAX_ISOLATED_FRAMING_RECORDS,
    )?;
    crate::marks::check_combined_slice_limits(document.marks(), &slice.0.marks)?;
    Ok(())
}
