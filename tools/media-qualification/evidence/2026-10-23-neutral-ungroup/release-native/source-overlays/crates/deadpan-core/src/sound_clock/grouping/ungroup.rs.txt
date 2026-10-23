//! Remove only an ordinary wrapper which every affected clock still treats
//! as introduced. Genuine historical processing owners remain in place.
use super::*;
use crate::NodeId;

pub(super) fn prepare(
    document: &ProjectDocument,
    request: &CommandRequest,
    removed: &NodeId,
) -> Result<Option<PreparedSoundGrouping>, EditError> {
    if document.audio_bindings.sound_clocks.is_empty() || removed == document.root() {
        return Ok(None);
    }
    let Some(beat) = document.nodes.get(removed) else {
        return Ok(None);
    };
    let NodeKind::Sequence { children } = &beat.kind else {
        return Ok(None);
    };
    let [child] = children.as_slice() else {
        return Ok(None);
    };
    if document
        .nodes
        .get(child)
        .is_none_or(|node| !matches!(node.kind, NodeKind::Source { .. } | NodeKind::Hold { .. }))
        || document.beat_sounds.contains_key(removed)
        || !beat.audio_treatments.is_empty()
        || beat.audio_edges != crate::AudioEdgePolicies::default()
        || !beat.audio_editorial_edges.is_empty()
        || beat.framing.is_some()
        || !beat.captions.is_empty()
        || !beat.cutaways.is_empty()
    {
        return Ok(None);
    }

    let mut work = 0;
    charge(&mut work, document.nodes.len())?;
    let parents: BTreeMap<_, _> = document
        .nodes
        .keys()
        .flat_map(|parent| document.children(parent).map(move |child| (child, parent)))
        .collect();
    let mut containing = BTreeSet::new();
    let mut ancestor = removed;
    loop {
        charge(&mut work, 1)?;
        containing.insert(ancestor.clone());
        let Some(next) = parents.get(ancestor) else {
            break;
        };
        ancestor = next;
    }
    let journals = &document.audio_bindings.sound_clocks;
    if !journals
        .values()
        .flat_map(|events| events.values())
        .any(|journal| containing.contains(journal.scope()))
    {
        return Ok(None);
    }
    // Charge all retained data before cloning. A genuine later capture of G
    // cannot be erased, even if its current treatment happens to be neutral.
    for journal in journals.values().flat_map(|events| events.values()) {
        charge(&mut work, journal.clocks().len())?;
        for reference in journal.clocks() {
            charge(&mut work, reference.introduced_groups().len())?;
            charge(&mut work, reference.split_work_count())?;
            charge(&mut work, reference.repeats().steps().len())?;
            if containing.contains(journal.scope())
                && (journal.scope() == removed || !reference.introduced_groups().contains(removed))
            {
                return Ok(None);
            }
        }
    }
    charge_capture(&mut work, document)?;
    let before = FrozenAudioLayout::capture_structural(document)?;
    let (ordinary, used) = before.ordinary_sound_ancestry(child, remaining(work)?)?;
    charge(&mut work, used)?;
    if !ordinary {
        return Ok(None);
    }
    let mut staged = document.clone();
    crate::command::reduce(&mut staged, &request.command, &request.new_revision)?;
    staged.audio_lineage.remove(removed);
    charge_capture(&mut work, &staged)?;
    let after = FrozenAudioLayout::capture_structural(&staged)?;

    let mut proofs = BTreeMap::new();
    let mut ancestry = BTreeMap::new();
    let mut journals = journals.clone();
    for (owner, events) in &mut journals {
        for journal in events.values_mut() {
            charge(&mut work, 1)?;
            if !containing.contains(journal.scope()) {
                continue;
            }
            if !matches!(
                before.nodes()[owner].kind,
                FrozenAudioKind::Source { .. } | FrozenAudioKind::Hold { .. }
            ) {
                return Ok(None);
            }
            let (ordinary, used) = before.ordinary_sound_ancestry(owner, remaining(work)?)?;
            charge(&mut work, used)?;
            if !ordinary {
                return Ok(None);
            }
            let mut references = Vec::with_capacity(journal.clocks().len());
            for reference in journal.clocks() {
                let layout = &document.audio_bindings.timings[reference.timing()];
                let ancestry_key = (reference.timing().clone(), reference.owner().clone());
                if !ancestry.contains_key(&ancestry_key) {
                    let (ordinary, used) =
                        layout.ordinary_sound_ancestry(reference.owner(), remaining(work)?)?;
                    charge(&mut work, used)?;
                    ancestry.insert(ancestry_key.clone(), ordinary);
                }
                if !ancestry[&ancestry_key]
                    || !matches!(
                        layout.nodes()[reference.owner()].kind,
                        FrozenAudioKind::Source { .. } | FrozenAudioKind::Hold { .. }
                    )
                    || !reference.repeats().steps().is_empty()
                {
                    return Ok(None);
                }
                charge(&mut work, reference.introduced_groups().len())?;
                let mut groups = reference.introduced_groups().clone();
                if !groups.remove(removed) {
                    return Err(invalid("ungroup lost its introduced sound relation"));
                }
                charge(&mut work, reference.introduced_groups().len())?;
                charge(&mut work, reference.split_work_count())?;
                let key = (
                    reference.timing().clone(),
                    reference.scope().clone(),
                    journal.scope().clone(),
                    (reference.introduced_groups().contains(journal.scope())
                        || layout.sound_uses_output_owner_proof(
                            &before,
                            (reference.scope(), journal.scope()),
                            (reference.owner(), owner),
                        ))
                    .then(|| (reference.owner().clone(), owner.clone())),
                    reference.repeats().clone(),
                    reference.introduced_groups().clone(),
                    reference.introduced_splits().to_vec(),
                );
                if !proofs.contains_key(&key) {
                    let old = layout.sound_owner_clock_correspondence_with_relations(
                        &before,
                        (reference.scope(), journal.scope()),
                        (reference.owner(), owner),
                        reference.repeats(),
                        (reference.introduced_groups(), reference.introduced_splits()),
                        remaining(work)?,
                    )?;
                    charge(&mut work, old.work())?;
                    let new = layout.sound_owner_clock_correspondence_with_relations(
                        &after,
                        (reference.scope(), journal.scope()),
                        (reference.owner(), owner),
                        reference.repeats(),
                        (&groups, reference.introduced_splits()),
                        remaining(work)?,
                    )?;
                    charge(&mut work, new.work())?;
                    if old.historical_node(removed).is_some()
                        || new.historical_node(removed).is_some()
                    {
                        return Err(invalid("ungroup removed a paired sound clock owner"));
                    }
                    let mut old_pairs = old.node_pairs();
                    let mut new_pairs = new.node_pairs();
                    loop {
                        charge(&mut work, 1)?;
                        match (old_pairs.next(), new_pairs.next()) {
                            (None, None) => break,
                            (Some(old), Some(new)) if old == new => {}
                            _ => {
                                return Err(invalid(
                                    "ungroup changed a retained sound clock alias",
                                ));
                            }
                        }
                    }
                    drop(old_pairs);
                    drop(new_pairs);
                    charge(&mut work, reference.introduced_groups().len())?;
                    charge(&mut work, reference.split_work_count())?;
                    proofs.insert(key.clone(), (old, new));
                }
                let (old, new) = &proofs[&key];
                if old.historical_node(owner) != Some(reference.owner())
                    || new.historical_node(owner) != Some(reference.owner())
                {
                    return Err(invalid("ungroup changed a retained sound clock owner"));
                }
                references.push(
                    SoundClockReference::new(
                        reference.timing().clone(),
                        reference.scope().clone(),
                        reference.owner().clone(),
                    )
                    .with_repeats(reference.repeats().clone())
                    .with_introduced_groups(groups)?
                    .with_introduced_splits(reference.introduced_splits().to_vec())?,
                );
            }
            *journal = SoundClockJournal::new(journal.scope().clone(), references)?;
        }
    }
    Ok(Some(PreparedSoundGrouping {
        project: document.project_id().clone(),
        revision: document.revision_id().clone(),
        command: request.command.clone(),
        journals,
    }))
}
