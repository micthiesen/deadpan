//! Atomic placement of a closed slice. Destination splits retain their original
//! clocks; replacement captures the suffix before removing selected contents.

use super::*;

enum Destination<'a> {
    Seam {
        index: usize,
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
    Children {
        resolved: SequenceChildrenPlan,
    },
}

struct Placement<'a> {
    /// The parent group's start in one already isolated Repeat play. Its
    /// boundaries come from that occurrence, and later plays and owners keep
    /// their own clocks.
    occurrence: Option<ProjectFrame>,
    parent: &'a NodeId,
    slice: &'a CapturedEditSlice,
    identities: &'a SlicePasteIdentities,
    splits: Option<&'a SplitIdentities>,
    timing: &'a AudioTimingId,
    destination: Destination<'a>,
}

impl<'a> Placement<'a> {
    fn resolve(
        document: &ProjectDocument,
        command: &'a Command,
        occurrence: Option<ProjectFrame>,
    ) -> Result<Self, EditError> {
        if occurrence.is_some() && matches!(command, Command::ReplaceSliceChildren { .. }) {
            return Err(invalid(
                "an occurrence paste places at a seam, an interior or a range",
            ));
        }
        let (parent, slice, identities, timing, splits, destination) = match command {
            Command::SpliceSlice {
                parent,
                index,
                slice,
                identities,
                timing,
            } => (parent, slice, identities, timing, None, {
                // Admits the seam's complete ancestry.
                crate::insert_time::source_splice::boundary_in(
                    document, parent, *index, occurrence,
                )?;
                Destination::Seam { index: *index }
            }),
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
                    resolved: document
                        .slice_splice_interior_in(parent, target, *at, slice, occurrence)?,
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
                    resolved: document.slice_replacement_in(parent, *range, slice, occurrence)?,
                },
            ),
            Command::ReplaceSliceChildren {
                parent,
                first,
                last,
                slice,
                identities,
                timing,
            } => (
                parent,
                slice,
                identities,
                timing,
                None,
                Destination::Children {
                    resolved: document.slice_children_replacement(parent, first, last, slice)?,
                },
            ),
            _ => return Err(invalid("slice placement requires a slice command")),
        };
        slice.check_destination(document)?;
        Ok(Self {
            occurrence,
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
            Destination::Children { .. } => 0,
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
        let suffix = match &placement.destination {
            Destination::Seam { .. } | Destination::Interior { .. } => {
                placement.slice.duration() != FrameDuration::ZERO
            }
            Destination::Replacement { range, .. } => {
                placement.slice.duration() != FrameDuration::ZERO && range.end().0 < total
            }
            Destination::Children { resolved } => {
                resolved.range.end().0 < total
                    && (resolved.range.duration() != FrameDuration::ZERO
                        || placement.slice.duration() != FrameDuration::ZERO)
            }
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
    apply_inner(document, command, context, false, None).map(|(document, _)| document)
}

/// Placement inside one already isolated Repeat play. `command` is the
/// `SpliceSlice`, `SpliceSliceAt` or `ReplaceSlice` in the play's group;
/// `origin` is that group's exact start in this occurrence. Every later owner,
/// including later plays and gaps of each enclosing Repeat, keeps its own
/// pre-edit clock.
pub(crate) fn apply_in_occurrence(
    document: &ProjectDocument,
    command: &Command,
    origin: ProjectFrame,
    context: crate::command::EditContext<'_>,
) -> Result<ProjectDocument, EditError> {
    apply_inner(document, command, context, false, Some(origin)).map(|(document, _)| document)
}

/// Set placement collects copied marks until every component is installed.
/// Finishing them against a partial destination could drop a valid final
/// Sequence-clock binding or split one logical mark across member copies.
pub(super) fn apply_detached_marks(
    document: &ProjectDocument,
    command: &Command,
    context: crate::command::EditContext<'_>,
) -> Result<(ProjectDocument, BTreeMap<MarkId, Mark>), EditError> {
    apply_inner(document, command, context, true, None)
}

fn apply_inner(
    document: &ProjectDocument,
    command: &Command,
    context: crate::command::EditContext<'_>,
    detach_marks: bool,
    occurrence: Option<ProjectFrame>,
) -> Result<(ProjectDocument, BTreeMap<MarkId, Mark>), EditError> {
    let placement = Placement::resolve(document, command, occurrence)?;
    if &placement.timing.allocation != context.allocation {
        return Err(invalid(
            "slice timing allocation must equal the new revision",
        ));
    }
    let total = document.duration()?.frames();
    let removed_duration = match &placement.destination {
        Destination::Replacement { range, .. } => range.duration().frames(),
        Destination::Children { resolved } => resolved.range.duration().frames(),
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

    let working = match (&placement.destination, &timings.split) {
        (Destination::Interior { target, at, .. }, Some(_)) => crate::split::apply(
            document,
            target,
            *at,
            placement.splits.expect("interior pools"),
            context,
        )?,
        (Destination::Replacement { range, .. }, Some(_)) => {
            crate::insert_time::sequence_range::split_endpoints_in(
                document,
                placement.parent,
                *range,
                placement.splits.expect("replacement pools"),
                context,
                placement.occurrence,
            )?
        }
        _ => document.clone(),
    };
    // Split can duplicate retained mark fragments and owner treatments. Charge
    // those actual bounded results together with the import before installation.
    check_combined(&working, placement.slice)?;
    let (first, end, removed) = match &placement.destination {
        Destination::Seam { index } => (*index, *index, Vec::new()),
        Destination::Interior { resolved, .. } => {
            (resolved.index + 1, resolved.index + 1, Vec::new())
        }
        Destination::Replacement { range, .. } => {
            let selected = crate::insert_time::sequence_range::selected_children_in(
                &working,
                placement.parent,
                *range,
                placement.occurrence,
            )?;
            (selected.first, selected.end, selected.nodes)
        }
        Destination::Children { resolved } => {
            let NodeKind::Sequence { children } = &working.nodes()[placement.parent].kind else {
                unreachable!("exact child query admitted a Sequence")
            };
            (
                resolved.first,
                resolved.end,
                children[resolved.first..resolved.end].to_vec(),
            )
        }
    };
    // Refuse a suffix that would cross a Repeat or Retime ancestor.
    if timings.suffix.is_some() {
        if placement.occurrence.is_some() {
            crate::insert_time::composite::occurrence_shifted_owners(
                &working,
                placement.parent,
                end,
            )?;
        } else {
            crate::insert_time::composite::check_suffix(&working, placement.parent, end)?;
        }
    }
    // Replace the split tree's child interval directly.
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
    if placement.slice.duration() != FrameDuration::ZERO
        || matches!(&placement.destination, Destination::Children { .. })
    {
        crate::audio_lineage::reconcile(&working, &mut result, command)?;
    }
    result.audio_lineage.extend(retained_lineage);
    let mut marks = std::mem::take(&mut result.marks);
    marks.retain(|id, _| !working.marks.contains_key(id));
    result.marks = if placement.slice.duration() == FrameDuration::ZERO
        && !matches!(&placement.destination, Destination::Children { .. })
    {
        working.marks.clone()
    } else {
        crate::marks::transform_marks(&working, &result, command)?
    };
    let detached = if detach_marks {
        marks
    } else {
        let copied = crate::marks::finish_slice_marks(&result, marks)?;
        result.marks.extend(copied);
        BTreeMap::new()
    };
    crate::audio_binding::prune(&mut result);
    result.validate()?;
    Ok((result, detached))
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
