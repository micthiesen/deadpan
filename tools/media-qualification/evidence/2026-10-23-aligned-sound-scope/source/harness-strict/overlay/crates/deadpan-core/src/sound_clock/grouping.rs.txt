//! Prepared admission for a fresh ordinary wrapper in retained sound scopes.

use std::collections::{BTreeMap, BTreeSet};

use super::{SoundClockJournal, SoundClockReference, SoundClocks};
use crate::{
    Command, CommandRequest, EditError, EditErrorCode, FrozenAudioKind, FrozenAudioLayout,
    MAX_DOCUMENT_NODES, NodeKind, ProjectDocument, ProjectId, RevisionId, SliceCaptureSelection,
};

/// Only preparation constructs this token. Its exact command and entry
/// identities prevent a proof for one group from admitting another edit.
pub(crate) struct PreparedSoundGrouping {
    project: ProjectId,
    revision: RevisionId,
    command: Command,
    journals: SoundClocks,
}

impl PreparedSoundGrouping {
    pub(crate) fn matches(&self, document: &ProjectDocument, command: &Command) -> bool {
        document.project_id() == &self.project
            && document.revision_id() == &self.revision
            && command == &self.command
    }

    pub(crate) fn install(
        &self,
        document: &mut ProjectDocument,
        command: &Command,
    ) -> Result<(), EditError> {
        if !self.matches(document, command) {
            return Err(invalid("prepared sound grouping does not match this edit"));
        }
        document.audio_bindings.sound_clocks = self.journals.clone();
        Ok(())
    }
}

pub(crate) fn prepare(
    document: &ProjectDocument,
    request: &CommandRequest,
) -> Result<Option<PreparedSoundGrouping>, EditError> {
    let Command::GroupSelection {
        parent,
        selection,
        label,
        identities,
        timing,
    } = &request.command
    else {
        return Ok(None);
    };
    if document.audio_bindings.sound_clocks.is_empty() {
        return Ok(None);
    }
    let plan = document.group_selection(parent, selection)?;
    if plan.required_split_ids != 0 {
        return Ok(None);
    }
    let (slot, selected) = match selection {
        SliceCaptureSelection::Child { node } => {
            let (slot, _) = crate::group_selection::child_range(document, parent, node)?;
            (slot, node)
        }
        SliceCaptureSelection::Children { first, last } => {
            let selected = document.sequence_children(parent, first, last)?;
            if selected.end != selected.first + 1 {
                return Ok(None);
            }
            (selected.first, first)
        }
        SliceCaptureSelection::Range { .. } => {
            let selected = crate::insert_time::sequence_range::selected_children(
                document, parent, plan.range,
            )?;
            if selected.nodes.len() != 1 {
                return Ok(None);
            }
            let NodeKind::Sequence { children } = &document.nodes[parent].kind else {
                unreachable!("group preflight admitted a Sequence")
            };
            (selected.first, &children[selected.first])
        }
    };
    if !matches!(
        document.nodes[selected].kind,
        NodeKind::Source { .. } | NodeKind::Hold { .. }
    ) {
        return Ok(None);
    }
    crate::group_selection::validate_group_label(label)?;
    crate::group_selection::validate_identities(document, identities, 0)?;
    crate::group_selection::validate_timing(document, timing, &request.new_revision)?;

    let mut work = 0;
    // This entry path determines every journal whose complete scope changes.
    // The node limit is charged before constructing the one parent index.
    charge(&mut work, document.nodes.len())?;
    let parents: BTreeMap<_, _> = document
        .nodes
        .keys()
        .flat_map(|parent| document.children(parent).map(move |child| (child, parent)))
        .collect();
    let mut containing = BTreeSet::new();
    let mut ancestor = parent;
    loop {
        charge(&mut work, 1)?;
        containing.insert(ancestor.clone());
        let Some(next) = parents.get(ancestor) else {
            break;
        };
        ancestor = next;
    }
    if !document
        .audio_bindings
        .sound_clocks
        .values()
        .flat_map(|events| events.values())
        .any(|journal| containing.contains(journal.scope()))
    {
        return Ok(None);
    }

    charge_capture(&mut work, document)?;
    let before = FrozenAudioLayout::capture_structural(document)?;
    let (ordinary, used) = before.ordinary_sound_ancestry(selected, remaining(work)?)?;
    charge(&mut work, used)?;
    if !ordinary {
        return Ok(None);
    }
    for journal in document
        .audio_bindings
        .sound_clocks
        .values()
        .flat_map(|events| events.values())
    {
        charge(&mut work, journal.clocks().len())?;
        for reference in journal.clocks() {
            charge(&mut work, reference.introduced_groups().len())?;
            charge(&mut work, reference.split_work_count())?;
            charge(&mut work, reference.repeats().steps().len())?;
        }
    }
    let mut grouped = document.clone();
    let NodeKind::Sequence { children } = &mut grouped.nodes.get_mut(parent).unwrap().kind else {
        unreachable!("group preflight admitted a Sequence")
    };
    children[slot] = identities.group.clone();
    grouped.nodes.insert(
        identities.group.clone(),
        crate::BeatNode::sequence(label, vec![selected.clone()]),
    );
    // Existing sound journals still describe the entry tree here. Capture the
    // new structure without recursively validating those not-yet-updated clocks.
    charge_capture(&mut work, &grouped)?;
    let after = FrozenAudioLayout::capture_structural(&grouped)?;
    let mut proofs = BTreeMap::new();
    let mut ancestry = BTreeMap::new();
    let mut journals = document.audio_bindings.sound_clocks.clone();
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
                charge(&mut work, reference.introduced_groups().len() + 1)?;
                let mut groups = reference.introduced_groups().clone();
                groups.insert(identities.group.clone());
                // Root-wrapper and output-owner proofs depend on the exact
                // owner pair; ordinary broad scopes share their complete proof.
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
                    if new.historical_node(&identities.group).is_some() {
                        return Err(invalid("group changed a retained sound clock alias"));
                    }
                    let mut old_pairs = old.node_pairs();
                    let mut new_pairs = new.node_pairs();
                    loop {
                        charge(&mut work, 1)?;
                        match (old_pairs.next(), new_pairs.next()) {
                            (None, None) => break,
                            (Some(old), Some(new)) if old == new => {}
                            _ => return Err(invalid("group changed a retained sound clock alias")),
                        }
                    }
                    drop(old_pairs);
                    drop(new_pairs);
                    charge(&mut work, reference.introduced_groups().len())?;
                    charge(&mut work, reference.split_work_count())?;
                    proofs.insert(key.clone(), (old, new));
                }
                let (old, new) = &proofs[&key];
                // Shared broad-scope proofs still have to prove every exact
                // live owner, including owners encountered only on cache hits.
                if old.historical_node(owner) != Some(reference.owner())
                    || new.historical_node(owner) != Some(reference.owner())
                {
                    return Err(invalid("group changed a retained sound clock owner"));
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

fn charge(work: &mut usize, additional: usize) -> Result<(), EditError> {
    *work = work
        .checked_add(additional)
        .filter(|used| *used <= MAX_DOCUMENT_NODES)
        .ok_or_else(exhausted)?;
    Ok(())
}
fn charge_capture(work: &mut usize, document: &ProjectDocument) -> Result<(), EditError> {
    charge(work, document.nodes().len())?;
    charge(work, document.audio_lineage().len())?;
    for node in document.nodes().values() {
        if let NodeKind::Repeat { iterations, .. } = &node.kind {
            charge(work, iterations.segment_count())?;
        }
    }
    Ok(())
}
fn remaining(work: usize) -> Result<usize, EditError> {
    MAX_DOCUMENT_NODES
        .checked_sub(work)
        .filter(|remaining| *remaining > 0)
        .ok_or_else(exhausted)
}
fn exhausted() -> EditError {
    EditError::new(
        EditErrorCode::LimitExceeded,
        "sound grouping proof work limit",
    )
}
fn invalid(message: &str) -> EditError {
    EditError::new(EditErrorCode::InvalidCommand, message)
}
