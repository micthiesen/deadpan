//! Retain shared processor clocks while one exact Repeat play acquires a copy.

use super::*;

pub(super) fn prepare(
    document: &mut ProjectDocument,
    mapping: &BTreeMap<NodeId, NodeId>,
    selected: &RepeatInstance,
) -> Result<(), EditError> {
    if document.audio_bindings.sound_clocks.is_empty() {
        return Ok(());
    }
    let NodeKind::Repeat {
        child, iterations, ..
    } = &document.nodes[&selected.node].kind
    else {
        return Err(invalid("sound isolation target is not a Repeat"));
    };
    if iterations.position(&selected.iteration).is_none()
        || document
            .overrides
            .get(&selected.node)
            .and_then(|entries| entries.get(&selected.iteration))
            .is_some()
    {
        return Err(invalid(
            "sound isolation requires one currently shared play",
        ));
    }
    let root = mapping
        .get(child)
        .ok_or_else(|| invalid("sound isolation lacks its complete default child"))?;
    let current = crate::FrozenAudioLayout::capture_structural(document)?;
    let mut journals = document.audio_bindings.sound_clocks.clone();
    let mut work = 0usize;
    for (owner, events) in &mut journals {
        for journal in events.values_mut() {
            let (contains, used) =
                current.sound_scope_contains(journal.scope(), &selected.node, remaining(work)?)?;
            charge(&mut work, used)?;
            if !contains {
                continue;
            }
            let mut references = Vec::new();
            for reference in journal.clocks() {
                charge(
                    &mut work,
                    reference.introduced_groups().len()
                        + reference.split_work_count()
                        + reference.isolation_work_count(),
                )?;
                let historical = &document.audio_bindings.timings[reference.timing()];
                let proof = historical.sound_owner_clock_correspondence_with_relations(
                    &current,
                    (reference.scope(), journal.scope()),
                    (reference.owner(), owner),
                    reference.repeats(),
                    reference.relations(),
                    remaining(work)?,
                )?;
                charge(&mut work, proof.work())?;
                let historical_repeat = proof
                    .historical_node(&selected.node)
                    .ok_or_else(|| invalid("sound isolation Repeat has no historical alias"))?;
                let mut groups = reference.introduced_groups().clone();
                for group in reference.introduced_groups() {
                    if let Some(copy) = mapping.get(group) {
                        groups.insert(copy.clone());
                    }
                }
                let mut splits = reference.introduced_splits().to_vec();
                for split in reference.introduced_splits() {
                    let copied = split
                        .pieces
                        .iter()
                        .filter(|piece| mapping.contains_key(&piece.partition))
                        .count();
                    if copied == 0 {
                        continue;
                    }
                    if copied != split.pieces.len() {
                        return Err(invalid("sound isolation copies an incomplete Split forest"));
                    }
                    let pieces = split
                        .pieces
                        .iter()
                        .map(|piece| {
                            Ok(crate::SoundClockSplitPiece {
                                partition: mapping[&piece.partition].clone(),
                                full_owner: mapping
                                    .get(&piece.full_owner)
                                    .ok_or_else(|| {
                                        invalid("sound isolation lost a Split complete owner")
                                    })?
                                    .clone(),
                            })
                        })
                        .collect::<Result<Vec<_>, EditError>>()?;
                    splits.push(crate::SoundClockSplit::new(
                        split.historical_node.clone(),
                        pieces,
                    )?);
                }
                let mut isolations = reference.introduced_isolations().to_vec();
                for relation in reference.introduced_isolations() {
                    if let Some(repeat) = mapping.get(&relation.live_repeat) {
                        let mut copy = relation.clone();
                        copy.live_repeat = repeat.clone();
                        copy.live_root = mapping
                            .get(&relation.live_root)
                            .ok_or_else(|| invalid("sound isolation lost an owned branch"))?
                            .clone();
                        isolations.push(copy);
                    }
                }
                isolations.push(crate::SoundClockIsolation {
                    live_repeat: selected.node.clone(),
                    historical_repeat: historical_repeat.clone(),
                    iteration: selected.iteration.clone(),
                    live_root: root.clone(),
                });
                references.push(
                    reference
                        .clone()
                        .with_introduced_groups(groups)?
                        .with_introduced_splits(splits)?
                        .with_introduced_isolations(isolations)?,
                );
            }
            *journal = crate::SoundClockJournal::new(journal.scope().clone(), references)?;
        }
    }
    document.audio_bindings.sound_clocks = journals;
    Ok(())
}

fn charge(work: &mut usize, amount: usize) -> Result<(), EditError> {
    *work = work
        .checked_add(amount)
        .filter(|work| *work <= crate::MAX_AUDIO_BINDING_ENTRIES)
        .ok_or_else(|| invalid("sound isolation proof work limit"))?;
    Ok(())
}

fn remaining(work: usize) -> Result<usize, EditError> {
    crate::MAX_AUDIO_BINDING_ENTRIES
        .checked_sub(work)
        .filter(|remaining| *remaining > 0)
        .map(|remaining| remaining.min(MAX_DOCUMENT_NODES))
        .ok_or_else(|| invalid("sound isolation proof work limit"))
}
