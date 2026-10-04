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
        | Command::ReplaceSliceChildren { timing, .. } => Some(timing),
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
        let mut moved: BTreeMap<(NodeId, NodeId), (crate::SoundClockCorrespondence, bool)> =
            BTreeMap::new();
        let mut historical: BTreeMap<
            (AudioTimingId, NodeId, NodeId),
            crate::SoundClockCorrespondence,
        > = BTreeMap::new();
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
                let (old_scope, live_scope) = match &previous {
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
                };
                let key = (old_scope.clone(), live_scope.clone());
                let (proof, changed_origin) = if let Some((proof, changed)) = moved.get(&key) {
                    (proof.clone(), *changed)
                } else {
                    let proof = self.before.sound_clock_correspondence(
                        &current,
                        &old_scope,
                        &live_scope,
                        budget(work)?,
                    )?;
                    work = work.checked_add(proof.work()).ok_or_else(exhausted)?;
                    let (old, old_work) = origin(&self.before, &old_scope, budget(work)?)?;
                    work = work.checked_add(old_work).ok_or_else(exhausted)?;
                    let (new, new_work) = origin(&current, &live_scope, budget(work)?)?;
                    work = work.checked_add(new_work).ok_or_else(exhausted)?;
                    let changed = old != new;
                    moved.insert(key, (proof.clone(), changed));
                    (proof, changed)
                };
                // Each event owner must retain its own identity in the paired
                // live subtree, even when another event already populated the
                // scope-pair cache. A same-shaped sibling is not an alias.
                if proof.historical_node(&owner) != Some(&owner) {
                    return Err(invalid("sound owner moved outside its captured scope"));
                }
                if let Some(previous) = &previous {
                    for reference in previous.clocks() {
                        let key = (
                            reference.timing().clone(),
                            reference.scope().clone(),
                            live_scope.clone(),
                        );
                        if !historical.contains_key(&key) {
                            let layout = &self.retained[reference.timing()];
                            let historical_proof = layout.sound_clock_correspondence(
                                &current,
                                reference.scope(),
                                &live_scope,
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
                    }
                }
                let journal = if changed_origin {
                    let reference = SoundClockReference::new(
                        self.timing.clone(),
                        old_scope.clone(),
                        owner.clone(),
                    );
                    Some(match previous {
                        Some(previous) => previous.with_appended(reference)?,
                        None => SoundClockJournal::new(live_scope.clone(), vec![reference])?,
                    })
                } else {
                    previous
                        .map(|previous| {
                            SoundClockJournal::new(live_scope.clone(), previous.clocks().to_vec())
                        })
                        .transpose()?
                };
                if let Some(journal) = journal {
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
