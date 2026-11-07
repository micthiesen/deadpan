//! Pure, immutable sound-clock proofs owned by one compiled plan. Media
//! admission and PCM remain the caller's responsibility on every read.

use super::*;

pub(super) type Correspondences = BTreeMap<
    NodeId,
    BTreeMap<deadpan_core::SoundId, Vec<Arc<deadpan_core::SoundClockCorrespondence>>>,
>;

type AssetInventory = BTreeMap<deadpan_core::AudioTimingId, BTreeSet<AssetId>>;
type Compiled = (Correspondences, AssetInventory, usize);

type ProofKey<'a> = (
    &'a deadpan_core::AudioTimingId,
    &'a NodeId,
    &'a NodeId,
    &'a deadpan_core::SoundClockRepeatMap,
    &'a BTreeSet<NodeId>,
    &'a [deadpan_core::SoundClockSplit],
    &'a [deadpan_core::SoundClockIsolation],
    Option<(&'a NodeId, &'a NodeId)>,
);

struct CompileWork(usize);

fn next_inventory_membership(count: usize) -> Result<usize, PlanError> {
    count
        .checked_add(1)
        .filter(|next| *next <= deadpan_core::MAX_AUDIO_BINDING_ENTRIES)
        .ok_or(PlanError::AudioQueryLimit("sound clock compile work"))
}

impl CompileWork {
    fn spend(&mut self, count: usize) -> Result<(), PlanError> {
        self.0 = self
            .0
            .checked_sub(count)
            .ok_or(PlanError::AudioQueryLimit("sound clock compile work"))?;
        Ok(())
    }

    fn remaining(&self) -> Result<usize, PlanError> {
        if self.0 == 0 {
            return Err(PlanError::AudioQueryLimit("sound clock compile work"));
        }
        Ok(self.0.min(deadpan_core::MAX_DOCUMENT_NODES))
    }
}

/// Return the remaining allowance for the current delivery proofs. Both
/// phases share the original per-plan aggregate ceiling.
pub(super) fn compile(plan: &RenderPlan) -> Result<Compiled, PlanError> {
    compile_bounded(plan, deadpan_core::MAX_AUDIO_BINDING_ENTRIES)
}

fn compile_bounded(plan: &RenderPlan, maximum_work: usize) -> Result<Compiled, PlanError> {
    if maximum_work == 0 || maximum_work > deadpan_core::MAX_AUDIO_BINDING_ENTRIES {
        return Err(PlanError::AudioQueryLimit("sound clock compile work"));
    }
    let journals = plan.audio_bindings.sound_clocks();
    if journals.is_empty() {
        return Ok((BTreeMap::new(), BTreeMap::new(), maximum_work));
    }
    let current = plan
        .sound_clock_layout
        .as_deref()
        .ok_or(PlanError::InvalidPlan(
            "beat sound clock has no live frozen layout",
        ))?;
    // This is the same aggregate ceiling used by Core's binding admission,
    // including its live snapshot and compact Repeat ancestry. Cached proof
    // storage is bounded by charged construction work, never by play count.
    let mut work = CompileWork(maximum_work);
    work.spend(current.nodes().len())?;
    work.spend(current.audio_lineage().len())?;
    for node in current.nodes().values() {
        if let deadpan_core::FrozenAudioKind::Repeat { iterations, .. } = &node.kind {
            work.spend(iterations.segment_count())?;
        }
    }
    let mut shared: BTreeMap<ProofKey<'_>, Arc<deadpan_core::SoundClockCorrespondence>> =
        BTreeMap::new();
    let mut assets_by_timing: BTreeMap<deadpan_core::AudioTimingId, BTreeSet<AssetId>> =
        BTreeMap::new();
    let mut inventory_memberships = 0usize;
    let mut owners = BTreeMap::new();
    for (owner, events) in journals {
        let mut sounds = BTreeMap::new();
        for (sound, journal) in events {
            let event = plan
                .beat_sounds
                .get(owner)
                .and_then(|events| events.get(sound))
                .ok_or(PlanError::InvalidPlan(
                    "sound clock has no matching live beat sound",
                ))?;
            let mut proofs = Vec::with_capacity(journal.clocks().len());
            for reference in journal.clocks() {
                work.spend(1)?;
                let asset = &event.source.asset;
                if !assets_by_timing
                    .get(reference.timing())
                    .is_some_and(|assets| assets.contains(asset))
                {
                    inventory_memberships = next_inventory_membership(inventory_memberships)?;
                    // Charge before creating the map/set entry or allocating its
                    // B-tree node. This membership cap shares the same ceiling
                    // as proof compilation; it is not an additional allowance.
                    work.spend(1)?;
                    assets_by_timing
                        .entry(reference.timing().clone())
                        .or_default()
                        .insert(asset.clone());
                }
                work.spend(reference.repeats().steps().len())?;
                work.spend(reference.introduced_groups().len())?;
                work.spend(reference.split_work_count())?;
                work.spend(reference.isolation_work_count())?;
                let historical = plan
                    .audio_bindings
                    .timings()
                    .get(reference.timing())
                    .ok_or(PlanError::InvalidPlan(
                        "beat sound clock refers to a missing frozen layout",
                    ))?;
                // Output-only owner proofs and introduced scope wrappers can
                // depend on the event owner. Full subtree proofs share their
                // exact clock/scope/relation key, then check each owner alias.
                let owners = (reference.introduced_groups().contains(journal.scope())
                    || historical.sound_uses_output_owner_proof(
                        current,
                        (reference.scope(), journal.scope()),
                        (reference.owner(), owner),
                    ))
                .then_some((reference.owner(), owner));
                let key = (
                    reference.timing(),
                    reference.scope(),
                    journal.scope(),
                    reference.repeats(),
                    reference.introduced_groups(),
                    reference.introduced_splits(),
                    reference.introduced_isolations(),
                    owners,
                );
                let proof = if let Some(proof) = shared.get(&key) {
                    Arc::clone(proof)
                } else {
                    let proof = historical.sound_owner_clock_correspondence_with_relations(
                        current,
                        (reference.scope(), journal.scope()),
                        (reference.owner(), owner),
                        reference.repeats(),
                        reference.relations(),
                        work.remaining()?,
                    )?;
                    work.spend(proof.work())?;
                    let proof = Arc::new(proof);
                    shared.insert(key, Arc::clone(&proof));
                    proof
                };
                if proof.historical_node(owner) != Some(reference.owner()) {
                    return Err(PlanError::InvalidPlan(
                        "beat sound clock owner is outside its proven scope",
                    ));
                }
                proofs.push(proof);
            }
            sounds.insert(sound.clone(), proofs);
        }
        owners.insert(owner.clone(), sounds);
    }
    Ok((owners, assets_by_timing, work.0))
}

#[cfg(test)]
#[path = "sound_clock_cache/tests.rs"]
mod tests;
