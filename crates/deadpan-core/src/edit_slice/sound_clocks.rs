//! Independent sound copies retain their processing clocks and sample history.

use super::*;

pub(super) fn capture(
    document: &ProjectDocument,
    parts: &[SlicePart],
    selected: &BTreeSet<NodeId>,
    timing: &AudioTimingId,
    state: &mut AudioBindingState,
) -> Result<(), EditError> {
    if !document.beat_sounds.keys().any(|id| selected.contains(id)) {
        return Ok(());
    }
    let live = FrozenAudioLayout::capture(document)?;
    let mut roots = BTreeMap::new();
    for part in parts {
        for id in crate::occurrence_edit::subtree_order(document, &part.root)? {
            if document.beat_sounds.contains_key(&id) {
                // A cropped sound owner needs its own interval lifecycle. Do not
                // silently capture the full sound behind a new crop wrapper.
                if part.mapping.start() != ProjectFrame(0)
                    || part.mapping.duration() != live.nodes()[&part.root].duration
                {
                    return Err(invalid("partial sound-bearing slice is not supported"));
                }
                roots.insert(id, part.root.clone());
            }
        }
    }
    let mut work = selected.len();
    let mut proofs = BTreeMap::new();
    let mut installed = BTreeSet::new();
    for (owner, root) in roots {
        let mut journals = BTreeMap::new();
        for sound in document.beat_sounds[&owner].keys() {
            let previous = document
                .audio_bindings
                .sound_clocks
                .get(&owner)
                .and_then(|events| events.get(sound));
            let scope = previous
                .map(SoundClockJournal::scope)
                .filter(|scope| selected.contains(*scope))
                .unwrap_or(&root)
                .clone();
            let mut references = Vec::new();
            if let Some(previous) = previous {
                for reference in previous.clocks() {
                    let historical = &document.audio_bindings.timings[reference.timing()];
                    let key = (
                        reference.timing().clone(),
                        reference.scope().clone(),
                        previous.scope().clone(),
                        reference.repeats().clone(),
                    );
                    if !proofs.contains_key(&key) {
                        let proof = historical.sound_clock_correspondence_with_repeats(
                            &live,
                            reference.scope(),
                            previous.scope(),
                            reference.repeats(),
                            budget(work)?,
                        )?;
                        work = work
                            .checked_add(proof.work())
                            .ok_or_else(|| limit("slice sound clock comparison overflow"))?;
                        proofs.insert(key.clone(), proof);
                    }
                    let proof = &proofs[&key];
                    if proof.historical_node(&owner) != Some(reference.owner()) {
                        return Err(invalid("slice sound clock owner differs from its history"));
                    }
                    // A selected child can narrow an old ordinary Sequence scope.
                    // Existing smaller scopes survive a copy of an outer group.
                    let historical_scope = proof
                        .historical_node(&scope)
                        .ok_or_else(|| invalid("slice sound clock scope is outside its history"))?;
                    references.push(
                        SoundClockReference::new(
                            reference.timing().clone(),
                            historical_scope.clone(),
                            reference.owner().clone(),
                        )
                        .with_repeats(reference.repeats().clone()),
                    );
                    if installed.insert(reference.timing().clone()) {
                        install(state, reference.timing(), historical)?;
                    }
                }
            }
            // The live source placement was implicit in its old journal. Make
            // it explicit before a fresh destination replaces that final clock.
            let (ancestors, used) = live.sound_placement_repeats(&scope, budget(work)?)?;
            work = work
                .checked_add(used)
                .ok_or_else(|| limit("slice sound ancestry work"))?;
            references.push(
                SoundClockReference::new(timing.clone(), scope.clone(), owner.clone())
                    .with_repeats(crate::SoundClockRepeatMap::new(
                        ancestors
                            .into_iter()
                            .map(|repeat| crate::SoundClockRepeatStep::Shared {
                                live_repeat: repeat.clone(),
                                historical_repeat: repeat,
                            })
                            .collect(),
                    )?),
            );
            journals.insert(sound.clone(), SoundClockJournal::new(scope, references)?);
        }
        state.sound_clocks.insert(owner, journals);
    }
    install(state, timing, &live)
}

fn install(
    state: &mut AudioBindingState,
    timing: &AudioTimingId,
    layout: &FrozenAudioLayout,
) -> Result<(), EditError> {
    if let Some(existing) = state.timings.get(timing) {
        if existing != layout {
            return Err(invalid(
                "slice timing identity names different sound clocks",
            ));
        }
    } else {
        state.timings.insert(timing.clone(), layout.clone());
    }
    Ok(())
}

fn budget(work: usize) -> Result<usize, EditError> {
    MAX_AUDIO_BINDING_ENTRIES
        .checked_sub(work)
        .filter(|remaining| *remaining > 0)
        .map(|remaining| remaining.min(MAX_DOCUMENT_NODES))
        .ok_or_else(|| limit("slice sound clock comparison limit"))
}
