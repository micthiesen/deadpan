//! Current delivery continuity, without authority over retained processing input.
use super::*;

pub(super) type Members = BTreeMap<(NodeId, deadpan_core::SoundId), Vec<Member>>;

#[derive(Debug, Clone)]
pub(super) struct Member {
    pub first: NodeId,
    pub full: NodeId,
    pub parent: usize,
    pub outer_repeats: usize,
    pub timing: deadpan_core::AudioTimingId,
    pub historical_full: NodeId,
    pub proof: Arc<deadpan_core::SoundClockCorrespondence>,
    /// A processor above these pieces consumes each complete copy, so Partition
    /// crops cannot divide it and each historical occurrence sounds only once.
    pub below_processor: bool,
    /// Repeats above that processor, which the proof's path index omits.
    pub repeats_above_proof: usize,
}

struct Work(usize);
impl Work {
    fn spend(&mut self, count: usize) -> Result<(), PlanError> {
        self.0 = self.0.checked_sub(count).ok_or(PlanError::AudioQueryLimit(
            "sound delivery forest compile work",
        ))?;
        Ok(())
    }
    fn remaining(&self) -> Result<usize, PlanError> {
        if self.0 == 0 {
            return Err(PlanError::AudioQueryLimit(
                "sound delivery forest compile work",
            ));
        }
        Ok(self.0.min(deadpan_core::MAX_DOCUMENT_NODES))
    }
}

pub(super) fn compile(plan: &RenderPlan, remaining_work: usize) -> Result<Members, PlanError> {
    let Some(current) = plan.sound_clock_layout.as_deref() else {
        return Ok(BTreeMap::new());
    };
    let mut work = Work(remaining_work);
    let mut members = BTreeMap::new();
    for (parent, node) in plan.nodes.iter().enumerate() {
        work.spend(1)?;
        let CompiledKind::Sequence { entries } = &node.kind else {
            continue;
        };
        let mut slot = 0;
        while slot < entries.len() {
            work.spend(1)?;
            let Some((first_full, start, _)) = partition(plan, entries[slot].child) else {
                slot += 1;
                continue;
            };
            if start != ExactRatio::ZERO {
                slot += 1;
                continue;
            }
            let duration = plan.nodes[first_full].extent;
            let mut end = ExactRatio::ZERO;
            let mut pieces = Vec::new();
            let mut unique = BTreeSet::new();
            for entry in &entries[slot..] {
                work.spend(1)?;
                let Some((full, begin, finish)) = partition(plan, entry.child) else {
                    break;
                };
                if begin != end
                    || finish.compare(begin) != std::cmp::Ordering::Greater
                    || finish.compare(duration) == std::cmp::Ordering::Greater
                    || plan.nodes[full].extent != duration
                    || !unique.insert(full)
                {
                    break;
                }
                pieces.push((entry.child, full));
                end = finish;
                if end == duration {
                    break;
                }
            }
            if pieces.len() >= 2 && end == duration {
                compile_forest(plan, current, parent, &pieces, &mut members, &mut work)?;
                slot += pieces.len();
            } else {
                slot += 1;
            }
        }
    }
    Ok(members)
}

fn partition(plan: &RenderPlan, index: usize) -> Option<(usize, ExactRatio, ExactRatio)> {
    let node = &plan.nodes[index];
    match node.kind {
        CompiledKind::Retime {
            child,
            start,
            scale,
            pitch,
            purpose: RetimePurpose::Partition,
        } if scale == ExactRatio::ONE
            && !pitch.processes(true)
            && node.preserve_input.is_none() =>
        {
            Some((child, start, start.checked_add(node.extent).ok()?))
        }
        _ => None,
    }
}

fn distance(
    plan: &RenderPlan,
    ancestor: usize,
    mut node: usize,
    work: &mut Work,
) -> Result<Option<usize>, PlanError> {
    for depth in 0..=deadpan_core::MAX_DOCUMENT_DEPTH {
        work.spend(1)?;
        if node == ancestor {
            return Ok(Some(depth));
        }
        let Some(parent) = plan.parents[node] else {
            return Ok(None);
        };
        node = parent;
    }
    Err(PlanError::AudioQueryLimit("sound delivery forest depth"))
}

fn compile_forest(
    plan: &RenderPlan,
    current: &deadpan_core::FrozenAudioLayout,
    parent: usize,
    pieces: &[(usize, usize)],
    members: &mut Members,
    work: &mut Work,
) -> Result<(), PlanError> {
    let first = &plan.nodes[pieces[0].0].inspection.id;
    let first_full = &plan.nodes[pieces[0].1].inspection.id;
    let Some(lineage) = current.audio_lineage().get(first_full) else {
        return Ok(());
    };
    for (_, full) in pieces {
        work.spend(1)?;
        if current
            .audio_lineage()
            .get(&plan.nodes[*full].inspection.id)
            != Some(lineage)
        {
            return Ok(());
        }
    }
    let first_treatments = &plan.nodes[pieces[0].1].audio_treatments;
    for (_, full) in pieces {
        work.spend(1)?;
        if &plan.nodes[*full].audio_treatments != first_treatments {
            return Ok(());
        }
    }
    let mut outer_repeats = 0;
    let mut repeats_above_proof = 0;
    let mut processed = false;
    let mut ancestor = Some(parent);
    while let Some(node) = ancestor {
        work.spend(1)?;
        let repeat = usize::from(matches!(plan.nodes[node].kind, CompiledKind::Repeat { .. }));
        outer_repeats += repeat;
        repeats_above_proof += usize::from(processed) * repeat;
        processed |= matches!(
            plan.nodes[node].kind,
            CompiledKind::Retime { purpose, .. } if purpose != RetimePurpose::Partition
        );
        ancestor = plan.parents[node];
    }
    for (owner, events) in &plan.beat_sounds {
        let Some(depth) = distance(plan, pieces[0].1, plan.by_id[owner], work)? else {
            continue;
        };
        for (sound, event) in events {
            work.spend(1)?;
            let Some(journal) = plan
                .audio_bindings
                .sound_clocks()
                .get(owner)
                .and_then(|events| events.get(sound))
            else {
                continue;
            };
            let reference = &journal.clocks()[0];
            let historical = &plan.audio_bindings.timings()[reference.timing()];
            // Anchor the full context to the recorded event path. Equal-looking
            // contexts and lineage alone cannot discover a historical alias.
            let mut historical_full = Some(reference.owner());
            for _ in 0..depth {
                work.spend(1)?;
                historical_full = historical_full.and_then(|node| historical.sound_parent(node));
            }
            let Some(historical_full) = historical_full else {
                continue;
            };
            let mut matched = Vec::new();
            for (_, full) in pieces {
                let full_id = &plan.nodes[*full].inspection.id;
                let mut selected = None;
                for (candidate, candidate_events) in &plan.beat_sounds {
                    work.spend(1)?;
                    let Some(candidate_event) = candidate_events.get(sound) else {
                        continue;
                    };
                    // Any later per-piece edit makes these separate sounds.
                    if candidate_event.source != event.source
                        || candidate_event.mapping != event.mapping
                        || candidate_event.offset != event.offset
                        || candidate_event.treatments != event.treatments
                        || candidate_event.start_edge != event.start_edge
                        || candidate_event.end_edge != event.end_edge
                        || candidate_event.overflow != event.overflow
                        || distance(plan, *full, plan.by_id[candidate], work)? != Some(depth)
                    {
                        continue;
                    }
                    let Some(candidate_journal) = plan
                        .audio_bindings
                        .sound_clocks()
                        .get(candidate)
                        .and_then(|events| events.get(sound))
                    else {
                        continue;
                    };
                    let candidate_reference = &candidate_journal.clocks()[0];
                    if candidate_reference.timing() != reference.timing()
                        || candidate_reference.scope() != reference.scope()
                        || candidate_reference.owner() != reference.owner()
                    {
                        continue;
                    }
                    let (repeats, used) = historical.sound_clock_context_repeats(
                        current,
                        (historical_full, full_id),
                        candidate_reference.repeats(),
                        candidate_reference.introduced_isolations(),
                        work.remaining()?,
                    );
                    work.spend(used)?;
                    let repeats = match repeats {
                        Ok(repeats) => repeats,
                        Err(error)
                            if error.code == deadpan_core::DocumentErrorCode::LimitExceeded =>
                        {
                            return Err(error.into());
                        }
                        Err(_) => continue,
                    };
                    let (attempt, used) = historical.sound_clock_delivery_correspondence_attempt(
                        current,
                        (historical_full, full_id),
                        &repeats,
                        work.remaining()?,
                    );
                    work.spend(used)?;
                    let proof = match attempt {
                        Ok(proof) => proof,
                        Err(error)
                            if error.code == deadpan_core::DocumentErrorCode::LimitExceeded =>
                        {
                            return Err(error.into());
                        }
                        Err(_) => continue,
                    };
                    if proof.historical_node(candidate) != Some(reference.owner()) {
                        continue;
                    }
                    if selected.is_some() {
                        selected = None;
                        break;
                    }
                    selected = Some((candidate.clone(), Arc::new(proof)));
                }
                let Some(selected) = selected else { break };
                matched.push(selected);
            }
            if matched.len() != pieces.len() {
                continue;
            }
            for ((owner, proof), (_, full)) in matched.into_iter().zip(pieces) {
                work.spend(1)?;
                members
                    .entry((owner, sound.clone()))
                    .or_default()
                    .push(Member {
                        first: first.clone(),
                        full: plan.nodes[*full].inspection.id.clone(),
                        parent,
                        outer_repeats,
                        timing: reference.timing().clone(),
                        historical_full: historical_full.clone(),
                        proof,
                        below_processor: processed,
                        repeats_above_proof,
                    });
            }
        }
    }
    Ok(())
}
