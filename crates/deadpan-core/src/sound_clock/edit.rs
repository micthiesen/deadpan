//! One transport around the structural reducer, using its caller-owned clock.

use std::collections::{BTreeMap, BTreeSet};

use super::{SoundClockJournal, SoundClockReference, SoundClocks};
use crate::{
    AudioTimingId, BeatSound, Command, EditError, EditErrorCode, ExactRatio, FrameRange,
    FrozenAudioLayout, InstancePath, MAX_DOCUMENT_NODES, NodeId, NodeKind, ProjectDocument,
    RevisionId, SoundId,
};

/// Only commands with an explicit clock and ordinary Sequence interval semantics
/// participate. Other time edits retain their existing truthful refusal.
pub(crate) fn timing(command: &Command) -> Option<&AudioTimingId> {
    match command {
        Command::InsertTime { timing, .. }
        | Command::SpliceSource { timing, .. }
        | Command::SpliceSourceAt { timing, .. }
        | Command::ReplaceSource { timing, .. }
        | Command::ReplaceSourceChildren { timing, .. }
        | Command::DeleteRipple { timing, .. }
        | Command::DeleteChildren { timing, .. }
        | Command::DeleteRange { timing, .. }
        | Command::MoveRange { timing, .. }
        | Command::SpliceSlice { timing, .. }
        | Command::SpliceSliceAt { timing, .. }
        | Command::ReplaceSlice { timing, .. }
        | Command::ReplaceSliceChildren { timing, .. }
        | Command::RepeatSelection { timing, .. }
        | Command::SetRepeatPlays { timing, .. }
        | Command::SetRepeatGaps { timing, .. } => Some(timing),
        _ => None,
    }
}

pub(crate) struct SoundClockEditCapture {
    events: BTreeMap<NodeId, BTreeMap<SoundId, BeatSound>>,
    journals: SoundClocks,
    retained: BTreeMap<AudioTimingId, FrozenAudioLayout>,
    before: FrozenAudioLayout,
    timing: AudioTimingId,
    removed: BTreeSet<NodeId>,
    repeat_edit: bool,
    introduced: Option<NodeId>,
}

impl SoundClockEditCapture {
    pub(crate) fn prepare(
        document: &ProjectDocument,
        command: &Command,
        allocation: &RevisionId,
    ) -> Result<Option<Self>, EditError> {
        let Some(timing) = timing(command).filter(|_| !document.beat_sounds.is_empty()) else {
            return Ok(None);
        };
        if &timing.allocation != allocation {
            return Err(invalid(
                "sound clock allocation must equal the new revision",
            ));
        }
        if document.audio_bindings.timings.contains_key(timing) {
            return Err(invalid("sound clock capture identity already exists"));
        }
        let before = FrozenAudioLayout::capture(document)?;
        if document.beat_sounds.contains_key(document.root()) {
            return Err(invalid("root-owned beat sounds cannot yet be transported"));
        }
        let journals = document.audio_bindings.sound_clocks.clone();
        let mut retained = BTreeMap::new();
        for events in journals.values() {
            for journal in events.values() {
                for reference in journal.clocks() {
                    retained
                        .entry(reference.timing().clone())
                        .or_insert_with(|| {
                            document.audio_bindings.timings[reference.timing()].clone()
                        });
                }
            }
        }
        Ok(Some(Self {
            events: document.beat_sounds.clone(),
            journals,
            retained,
            before,
            timing: timing.clone(),
            removed: removed_owners(document, command)?,
            repeat_edit: matches!(
                command,
                Command::RepeatSelection { .. }
                    | Command::SetRepeatPlays { .. }
                    | Command::SetRepeatGaps { .. }
            ),
            introduced: match command {
                Command::RepeatSelection { identities, .. } => Some(identities.repeat.clone()),
                _ => None,
            },
        }))
    }

    pub(crate) fn detach(&self, document: &mut ProjectDocument) {
        document.beat_sounds.clear();
        document.audio_bindings.sound_clocks.clear();
        crate::audio_binding_lifecycle::prune(document);
    }

    pub(crate) fn restore(mut self, document: &mut ProjectDocument) -> Result<(), EditError> {
        // Imported events already belong to the result. Never replace that map
        // with the entry snapshot, and never transport their newly born clocks.
        let current = FrozenAudioLayout::capture(document)?;
        let mut moved = BTreeMap::new();
        let mut historical = BTreeMap::new();
        let mut work = 0usize;
        for (owner, events) in self.events {
            if !document.nodes.contains_key(&owner) {
                if self.removed.contains(&owner) {
                    continue;
                }
                return Err(invalid(
                    "sound owner disappeared without whole-owner deletion",
                ));
            }
            let (top, used) =
                self.before
                    .branch_below(self.before.root(), &owner, budget(work)?)?;
            work = work.checked_add(used).ok_or_else(exhausted)?;
            if document
                .beat_sounds
                .insert(owner.clone(), events.clone())
                .is_some()
            {
                return Err(invalid(
                    "imported beat sound owner collides with a surviving owner",
                ));
            }
            let mut journals = self.journals.remove(&owner).unwrap_or_default();
            for id in events.keys() {
                let previous = journals.remove(id);
                let (old_scope, live_scope) = if self.repeat_edit {
                    let (scope, used) =
                        self.before.sound_processing_scope(&owner, budget(work)?)?;
                    work = work.checked_add(used).ok_or_else(exhausted)?;
                    (scope.clone(), scope)
                } else {
                    match &previous {
                        Some(previous) => {
                            if !document.nodes.contains_key(previous.scope()) {
                                return Err(invalid(
                                    "sound owner scope disappeared without whole-owner deletion",
                                ));
                            }
                            (previous.scope().clone(), previous.scope().clone())
                        }
                        None => {
                            let live_scope =
                                current.branch_below(current.root(), &owner, budget(work)?)?;
                            work = work.checked_add(live_scope.1).ok_or_else(exhausted)?;
                            (top.clone(), live_scope.0)
                        }
                    }
                };
                let (ancestors, used) =
                    current.sound_placement_repeats(&live_scope, budget(work)?)?;
                work = work.checked_add(used).ok_or_else(exhausted)?;
                let before_map = crate::SoundClockRepeatMap::new(
                    ancestors
                        .iter()
                        .map(|repeat| {
                            if self.introduced.as_ref() == Some(repeat) {
                                crate::SoundClockRepeatStep::Introduced {
                                    live_repeat: repeat.clone(),
                                    plays: introduced_plays(&current, repeat).clone(),
                                }
                            } else {
                                crate::SoundClockRepeatStep::Shared {
                                    live_repeat: repeat.clone(),
                                    historical_repeat: repeat.clone(),
                                }
                            }
                        })
                        .collect(),
                )?;
                let key = (old_scope.clone(), live_scope.clone());
                let (proof, changed_origin) = if let Some((proof, changed)) = moved.get(&key) {
                    (crate::SoundClockCorrespondence::clone(proof), *changed)
                } else {
                    let proof = self.before.sound_clock_correspondence_with_repeats(
                        &current,
                        &old_scope,
                        &live_scope,
                        &before_map,
                        budget(work)?,
                    )?;
                    work = work.checked_add(proof.work()).ok_or_else(exhausted)?;
                    let changed = if !ancestors.is_empty() {
                        let (same, used) = self.before.sound_placement_unchanged(
                            &current,
                            &old_scope,
                            budget(work)?,
                        )?;
                        work = work.checked_add(used).ok_or_else(exhausted)?;
                        !same
                    } else {
                        let (old, used) = origin(&self.before, &old_scope, budget(work)?)?;
                        work = work.checked_add(used).ok_or_else(exhausted)?;
                        let (new, used) = origin(&current, &live_scope, budget(work)?)?;
                        work = work.checked_add(used).ok_or_else(exhausted)?;
                        old != new
                    };
                    moved.insert(key, (proof.clone(), changed));
                    (proof, changed)
                };
                // Each event owner must retain its own identity in the paired
                // live subtree, even when another event already populated the
                // scope-pair cache. A same-shaped sibling is not an alias.
                if proof.historical_node(&owner) != Some(&owner) {
                    return Err(invalid("sound owner moved outside its captured scope"));
                }
                let mut references = Vec::new();
                if let Some(previous) = &previous {
                    for reference in previous.clocks() {
                        let key = (
                            reference.timing().clone(),
                            reference.scope().clone(),
                            previous.scope().clone(),
                            reference.repeats().clone(),
                        );
                        if !historical.contains_key(&key) {
                            let layout = &self.retained[reference.timing()];
                            let historical_proof = layout.sound_clock_correspondence_with_repeats(
                                &self.before,
                                reference.scope(),
                                previous.scope(),
                                reference.repeats(),
                                budget(work)?,
                            )?;
                            work = work
                                .checked_add(historical_proof.work())
                                .ok_or_else(exhausted)?;
                            historical.insert(key.clone(), historical_proof);
                        }
                        let historical_proof = historical
                            .get(&key)
                            .expect("historical sound clock proof was inserted");
                        if historical_proof.historical_node(&owner) != Some(reference.owner()) {
                            return Err(invalid("sound owner processing subtree changed"));
                        }
                        let scope =
                            historical_proof
                                .historical_node(&old_scope)
                                .ok_or_else(|| {
                                    invalid("sound clock cannot narrow outside its captured scope")
                                })?;
                        let repeats = crate::SoundClockRepeatMap::new(ancestors.iter().map(|repeat| {
                            if self.introduced.as_ref() == Some(repeat) {
                                Ok(crate::SoundClockRepeatStep::Introduced { live_repeat: repeat.clone(), plays: introduced_plays(&current, repeat).clone() })
                            } else if let Some(step) = reference.repeats().steps().iter().find(|step| matches!(step,
                                crate::SoundClockRepeatStep::Introduced { live_repeat, .. } if live_repeat == repeat)) {
                                Ok(step.clone())
                            } else {
                                Ok(crate::SoundClockRepeatStep::Shared {
                                    live_repeat: repeat.clone(),
                                    historical_repeat: historical_proof.historical_node(repeat)
                                        .ok_or_else(|| invalid("sound Repeat has no proven historical alias"))?.clone(),
                                })
                            }
                        }).collect::<Result<Vec<_>, EditError>>()?)?;
                        references.push(
                            SoundClockReference::new(
                                reference.timing().clone(),
                                scope.clone(),
                                reference.owner().clone(),
                            )
                            .with_repeats(repeats),
                        );
                    }
                }
                if changed_origin {
                    references.push(
                        SoundClockReference::new(
                            self.timing.clone(),
                            old_scope.clone(),
                            owner.clone(),
                        )
                        .with_repeats(before_map),
                    );
                }
                if !references.is_empty() {
                    let journal = SoundClockJournal::new(live_scope, references)?;
                    for reference in journal.clocks() {
                        let layout = if reference.timing() == &self.timing {
                            &self.before
                        } else {
                            &self.retained[reference.timing()]
                        };
                        install_layout(document, reference.timing(), layout)?;
                    }
                    document
                        .audio_bindings
                        .sound_clocks
                        .entry(owner.clone())
                        .or_default()
                        .insert(id.clone(), journal);
                }
            }
        }
        Ok(())
    }
}

fn introduced_plays<'a>(
    layout: &'a FrozenAudioLayout,
    repeat: &NodeId,
) -> &'a crate::IterationOrder {
    let crate::FrozenAudioKind::Repeat { iterations, .. } = &layout.nodes()[repeat].kind else {
        unreachable!("admitted Repeat ancestry")
    };
    iterations
}

fn install_layout(
    document: &mut ProjectDocument,
    timing: &AudioTimingId,
    layout: &FrozenAudioLayout,
) -> Result<(), EditError> {
    if let Some(existing) = document.audio_bindings.timings.get(timing) {
        if existing != layout {
            return Err(invalid(
                "sound clock identity names a different structural layout",
            ));
        }
    } else {
        document
            .audio_bindings
            .timings
            .insert(timing.clone(), layout.clone());
    }
    Ok(())
}

fn origin(
    layout: &FrozenAudioLayout,
    top: &NodeId,
    maximum_work: usize,
) -> Result<(ExactRatio, usize), EditError> {
    // A direct root child has no Repeat ancestry. This is its geometric origin,
    // used only to decide whether transport is needed, never its audible extent.
    let projection = layout.project(
        &InstancePath {
            node: top.clone(),
            repeats: vec![],
        },
        ExactRatio::ZERO,
        None,
        maximum_work,
    )?;
    Ok((projection.origin, projection.work))
}

fn removed_owners(
    document: &ProjectDocument,
    command: &Command,
) -> Result<BTreeSet<NodeId>, EditError> {
    let roots = match command {
        Command::DeleteRipple { node, .. } => {
            if !document.nodes.contains_key(node) {
                return Err(invalid("deleted sound subtree is missing"));
            }
            vec![node.clone()]
        }
        Command::SetRepeatPlays { node, plays, .. } => {
            let Some(crate::BeatNode {
                kind: NodeKind::Repeat { iterations, .. },
                ..
            }) = document.nodes().get(node)
            else {
                return Err(invalid("sound clock count edit needs a Repeat"));
            };
            let entries = document
                .overrides()
                .get(node)
                .map_or(0, crate::PlayOverrides::len)
                + document
                    .gap_overrides()
                    .get(node)
                    .map_or(0, crate::PlayOverrides::len);
            if entries
                .checked_mul(iterations.segment_count())
                .is_none_or(|work| work > MAX_DOCUMENT_NODES)
            {
                return Err(exhausted());
            }
            document
                .overrides()
                .get(node)
                .into_iter()
                .chain(document.gap_overrides().get(node))
                .flat_map(|entries| entries.iter())
                .filter(|(play, _)| {
                    iterations
                        .position(play)
                        .is_some_and(|position| position >= *plays)
                })
                .map(|(_, root)| root.clone())
                .collect()
        }
        Command::SetRepeatGaps { node, .. } => document
            .gap_overrides()
            .get(node)
            .into_iter()
            .flat_map(|entries| entries.iter())
            .map(|(_, root)| root.clone())
            .collect(),
        Command::DeleteChildren {
            parent,
            first,
            last,
            ..
        }
        | Command::ReplaceSourceChildren {
            parent,
            first,
            last,
            ..
        }
        | Command::ReplaceSliceChildren {
            parent,
            first,
            last,
            ..
        } => {
            let span = document.sequence_children(parent, first, last)?;
            let NodeKind::Sequence { children } = &document.nodes[parent].kind else {
                unreachable!()
            };
            children[span.first..span.end].to_vec()
        }
        Command::DeleteRange { parent, range, .. }
        | Command::ReplaceSource { parent, range, .. }
        | Command::ReplaceSlice { parent, range, .. } => complete_roots(document, parent, *range)?,
        _ => vec![],
    };
    let mut removed = BTreeSet::new();
    for root in roots {
        removed.extend(crate::occurrence_edit::subtree_order(document, &root)?);
    }
    Ok(removed)
}

fn complete_roots(
    document: &ProjectDocument,
    parent: &NodeId,
    range: FrameRange,
) -> Result<Vec<NodeId>, EditError> {
    let mut start = document.source_splice_boundary(parent, 0)?;
    let NodeKind::Sequence { children } = &document.nodes[parent].kind else {
        unreachable!()
    };
    let durations = document.durations()?;
    let mut roots = Vec::new();
    for child in children {
        let end = crate::ProjectFrame(
            start
                .0
                .checked_add(durations[child].frames())
                .ok_or_else(exhausted)?,
        );
        if start >= range.start() && end <= range.end() {
            roots.push(child.clone());
        }
        start = end;
    }
    Ok(roots)
}

fn budget(work: usize) -> Result<usize, EditError> {
    MAX_DOCUMENT_NODES
        .checked_sub(work)
        .filter(|remaining| *remaining > 0)
        .ok_or_else(exhausted)
}
fn exhausted() -> EditError {
    EditError::new(
        EditErrorCode::LimitExceeded,
        "sound clock transport work limit",
    )
}
fn invalid(message: &str) -> EditError {
    EditError::new(EditErrorCode::InvalidCommand, message)
}
