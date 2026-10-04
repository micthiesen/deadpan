//! Injective typed renaming of current owners and complete historical layouts.
//! Repeat families are joined only by explicit binding relationships.

use super::*;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum RepeatKey {
    Live(NodeId),
    Historical(AudioTimingId, NodeId),
}

#[derive(Default)]
pub(super) struct Inventory {
    aliases: BTreeSet<(AudioTimingId, NodeId)>,
    lineages: BTreeSet<AudioLineageId>,
    keys: BTreeMap<RepeatKey, usize>,
    parents: Vec<usize>,
    sizes: Vec<usize>,
    intervals: Vec<(usize, RevisionId, u64, u64)>,
    work: usize,
}

impl Inventory {
    pub(super) fn new(slice: &SliceWire) -> Result<Self, EditError> {
        let mut result = Self::default();
        result.charge(slice.nodes.len() + slice.marks.len())?;
        for (id, node) in &slice.nodes {
            if let NodeKind::Repeat { iterations, .. } = &node.kind {
                result.order(RepeatKey::Live(id.clone()), iterations)?;
            }
        }
        result
            .lineages
            .extend(slice.audio_lineage.values().cloned());
        for (timing, layout) in &slice.audio_bindings.timings {
            result.charge(layout.nodes().len() + layout.audio_lineage().len())?;
            for (id, node) in layout.nodes() {
                result.aliases.insert((timing.clone(), id.clone()));
                if let FrozenAudioKind::Repeat { iterations, .. } = &node.kind {
                    result.order(
                        RepeatKey::Historical(timing.clone(), id.clone()),
                        iterations,
                    )?;
                }
            }
            result
                .lineages
                .extend(layout.audio_lineage().values().cloned());
            for entries in layout
                .overrides()
                .values()
                .chain(layout.gap_overrides().values())
            {
                result.charge(entries.len())?;
            }
        }
        for (_, _, binding) in slice.audio_bindings.owners() {
            for placement in binding.placements() {
                result.charge(placement.entry_count())?;
                let timing = &placement.reference.timing;
                for argument in &placement.arguments {
                    result.value(timing, &argument.reference_repeat, &argument.value)?;
                }
                if let Some(value) = &placement.gap_after {
                    result.value(timing, &placement.reference.physical, value)?;
                }
                for clause in &placement.births {
                    let live = result.key(&RepeatKey::Live(clause.repeat.clone()))?;
                    match &clause.survivors {
                        AudioBirthSurvivors::CapturedRepeat { repeat } => {
                            let historical = result
                                .key(&RepeatKey::Historical(timing.clone(), repeat.clone()))?;
                            result.join(live, historical);
                        }
                        AudioBirthSurvivors::Run {
                            allocation,
                            first,
                            count,
                        } => result.interval(live, allocation, *first, *count)?,
                    }
                }
            }
        }
        if !slice.audio_bindings.sound_clocks.is_empty() {
            let context = CapturedEditSlice::context_for(slice)?;
            result.charge(context.nodes().len())?;
            let live = FrozenAudioLayout::capture(&context)?;
            let mut proofs = BTreeMap::new();
            for (owner, journals) in &slice.audio_bindings.sound_clocks {
                for journal in journals.values() {
                    for reference in journal.clocks() {
                        let key = (
                            reference.timing().clone(),
                            reference.scope().clone(),
                            journal.scope().clone(),
                        );
                        if !proofs.contains_key(&key) {
                            let historical = &slice.audio_bindings.timings[reference.timing()];
                            let remaining = MAX_AUDIO_BINDING_ENTRIES
                                .checked_sub(result.work)
                                .filter(|remaining| *remaining > 0)
                                .ok_or_else(|| limit("slice sound clock identity work limit"))?;
                            let proof = historical.sound_clock_correspondence(
                                &live,
                                reference.scope(),
                                journal.scope(),
                                remaining.min(MAX_DOCUMENT_NODES),
                            )?;
                            result.charge(proof.work())?;
                            // Only proven corresponding Repeat definitions share a
                            // fresh play family. Equal old strings alone prove none.
                            for (current, old) in proof.node_pairs() {
                                if matches!(
                                    live.nodes()[current].kind,
                                    FrozenAudioKind::Repeat { .. }
                                ) {
                                    let current = result.key(&RepeatKey::Live(current.clone()))?;
                                    let old = result.key(&RepeatKey::Historical(
                                        reference.timing().clone(),
                                        old.clone(),
                                    ))?;
                                    result.join(current, old);
                                }
                            }
                            proofs.insert(key.clone(), proof);
                        }
                        if proofs[&key].historical_node(owner) != Some(reference.owner()) {
                            return Err(invalid(
                                "slice sound owner has a different historical alias",
                            ));
                        }
                    }
                }
            }
        }
        for mark in slice.marks.values() {
            result.charge(mark.binding_count())?;
            for binding in mark.bindings() {
                if binding.state == MarkState::Bound
                    && let Anchor::Occurrence { instance, .. } = binding.coordinate
                {
                    for step in instance.repeats {
                        result.charge(1)?;
                        let key = result.key(&RepeatKey::Live(step.node))?;
                        result.interval(
                            key,
                            &step.iteration.allocation,
                            step.iteration.ordinal,
                            1,
                        )?;
                    }
                }
            }
        }
        result.charge(result.lineages.len())?;
        // Prove every scoped union fits before constructing a renamed payload.
        result.compact_ranges()?;
        Ok(result)
    }

    fn charge(&mut self, amount: usize) -> Result<(), EditError> {
        self.work = self
            .work
            .checked_add(amount)
            .filter(|count| *count <= MAX_AUDIO_BINDING_ENTRIES)
            .ok_or_else(|| limit("slice current and historical identity/work limit"))?;
        Ok(())
    }
    fn order(&mut self, key: RepeatKey, order: &IterationOrder) -> Result<(), EditError> {
        self.charge(order.segment_count())?;
        let index = self.parents.len();
        if self.keys.insert(key, index).is_some() {
            return Err(invalid("duplicate slice Repeat scope"));
        }
        self.parents.push(index);
        self.sizes.push(1);
        for (allocation, first, count) in order.segments() {
            self.interval(index, allocation, first, count)?;
        }
        Ok(())
    }
    fn key(&self, key: &RepeatKey) -> Result<usize, EditError> {
        self.keys
            .get(key)
            .copied()
            .ok_or_else(|| invalid("slice Repeat relationship is outside its closure"))
    }
    fn interval(
        &mut self,
        index: usize,
        allocation: &RevisionId,
        first: u32,
        count: u32,
    ) -> Result<(), EditError> {
        let end = u64::from(first) + u64::from(count);
        if count == 0 || end > u64::from(u32::MAX) + 1 {
            return Err(invalid("slice play interval is invalid"));
        }
        if self.intervals.len() >= MAX_AUDIO_BINDING_ENTRIES {
            return Err(limit("slice compact play interval limit"));
        }
        self.intervals
            .push((index, allocation.clone(), u64::from(first), end));
        Ok(())
    }
    fn root(&self, mut index: usize) -> usize {
        while self.parents[index] != index {
            index = self.parents[index];
        }
        index
    }
    fn join(&mut self, a: usize, b: usize) {
        let mut a = self.root(a);
        let mut b = self.root(b);
        if a != b {
            // Weighted union bounds lookup depth logarithmically without an
            // identity-sized expansion or recursion.
            if self.sizes[a] < self.sizes[b] {
                std::mem::swap(&mut a, &mut b);
            }
            self.parents[b] = a;
            self.sizes[a] += self.sizes[b];
        }
    }
    fn value(
        &mut self,
        timing: &AudioTimingId,
        reference: &NodeId,
        value: &AudioRepeatValue,
    ) -> Result<(), EditError> {
        let historical = self.key(&RepeatKey::Historical(timing.clone(), reference.clone()))?;
        match value {
            AudioRepeatValue::Live { repeat } => {
                let live = self.key(&RepeatKey::Live(repeat.clone()))?;
                self.join(live, historical);
            }
            AudioRepeatValue::Captured { iteration } => {
                self.interval(historical, &iteration.allocation, iteration.ordinal, 1)?
            }
        }
        Ok(())
    }
    pub(super) fn alias_count(&self) -> usize {
        self.aliases.len() + self.lineages.len()
    }

    fn compact_ranges(
        &self,
    ) -> Result<BTreeMap<(usize, RevisionId), Vec<RenamedRange>>, EditError> {
        let mut grouped: BTreeMap<(usize, RevisionId), Vec<(u64, u64)>> = BTreeMap::new();
        for (key, allocation, start, end) in &self.intervals {
            grouped
                .entry((self.root(*key), allocation.clone()))
                .or_default()
                .push((*start, *end));
        }
        let mut next = BTreeMap::<usize, u64>::new();
        let mut output = BTreeMap::new();
        for (key, mut intervals) in grouped {
            intervals.sort_unstable();
            let mut merged: Vec<(u64, u64)> = Vec::new();
            for (start, end) in intervals {
                if let Some(last) = merged.last_mut()
                    && start <= last.1
                {
                    last.1 = last.1.max(end);
                } else {
                    merged.push((start, end));
                }
            }
            let ordinal = next.entry(key.0).or_default();
            let mut ranges = Vec::with_capacity(merged.len());
            for (start, end) in merged {
                let first = *ordinal;
                *ordinal = ordinal
                    .checked_add(end - start)
                    .filter(|value| *value <= u64::from(u32::MAX) + 1)
                    .ok_or_else(|| limit("slice scoped play union exceeds fresh ordinal space"))?;
                ranges.push(RenamedRange { start, end, first });
            }
            output.insert(key, ranges);
        }
        Ok(output)
    }
}

struct RenamedRange {
    start: u64,
    end: u64,
    first: u64,
}
struct Renamer {
    inventory: Inventory,
    ranges: BTreeMap<(usize, RevisionId), Vec<RenamedRange>>,
    nodes: BTreeMap<NodeId, NodeId>,
    aliases: BTreeMap<(AudioTimingId, NodeId), NodeId>,
    lineages: BTreeMap<AudioLineageId, AudioLineageId>,
    timings: BTreeMap<AudioTimingId, AudioTimingId>,
    allocation: RevisionId,
}

impl Renamer {
    fn live(&self, id: &NodeId) -> Result<NodeId, EditError> {
        self.nodes
            .get(id)
            .cloned()
            .ok_or_else(|| invalid("slice live reference is outside copied content"))
    }
    fn historical(&self, timing: &AudioTimingId, id: &NodeId) -> Result<NodeId, EditError> {
        self.aliases
            .get(&(timing.clone(), id.clone()))
            .cloned()
            .ok_or_else(|| invalid("slice historical alias is missing"))
    }
    fn play(&self, key: RepeatKey, id: &IterationId) -> Result<IterationId, EditError> {
        let family = self.inventory.root(self.inventory.key(&key)?);
        let ranges = self
            .ranges
            .get(&(family, id.allocation.clone()))
            .ok_or_else(|| invalid("slice play allocation missing"))?;
        let ordinal = u64::from(id.ordinal);
        let position = ranges.partition_point(|range| range.start <= ordinal);
        let range = position
            .checked_sub(1)
            .and_then(|index| ranges.get(index))
            .filter(|range| ordinal < range.end)
            .ok_or_else(|| invalid("slice play identity is outside compact support"))?;
        let ordinal = u32::try_from(range.first + ordinal - range.start)
            .map_err(|_| limit("slice play rename overflow"))?;
        Ok(IterationId {
            allocation: self.allocation.clone(),
            ordinal,
        })
    }
    fn order(&self, key: RepeatKey, order: &IterationOrder) -> Result<IterationOrder, EditError> {
        let runs = order
            .segments()
            .map(|(allocation, first, count)| {
                let start = self.play(
                    key.clone(),
                    &IterationId {
                        allocation: allocation.clone(),
                        ordinal: first,
                    },
                )?;
                Ok((start.allocation, start.ordinal, count))
            })
            .collect::<Result<Vec<_>, EditError>>()?;
        Ok(IterationOrder::from_segments(runs)?)
    }
    fn overrides(
        &self,
        input: &BTreeMap<NodeId, PlayOverrides>,
        timing: Option<&AudioTimingId>,
    ) -> Result<BTreeMap<NodeId, PlayOverrides>, EditError> {
        input
            .iter()
            .map(|(owner, entries)| {
                let renamed = match timing {
                    Some(timing) => self.historical(timing, owner)?,
                    None => self.live(owner)?,
                };
                let key = match timing {
                    Some(timing) => RepeatKey::Historical(timing.clone(), owner.clone()),
                    None => RepeatKey::Live(owner.clone()),
                };
                let entries = entries
                    .iter()
                    .map(|(iteration, child)| {
                        Ok(PlayOverride {
                            iteration: self.play(key.clone(), iteration)?,
                            root: match timing {
                                Some(timing) => self.historical(timing, child)?,
                                None => self.live(child)?,
                            },
                        })
                    })
                    .collect::<Result<Vec<_>, EditError>>()?;
                Ok((renamed, PlayOverrides::try_from(entries)?))
            })
            .collect()
    }
    fn lineage(
        &self,
        input: &BTreeMap<NodeId, AudioLineageId>,
        timing: Option<&AudioTimingId>,
    ) -> Result<BTreeMap<NodeId, AudioLineageId>, EditError> {
        input
            .iter()
            .map(|(owner, lineage)| {
                Ok((
                    match timing {
                        Some(timing) => self.historical(timing, owner)?,
                        None => self.live(owner)?,
                    },
                    self.lineages
                        .get(lineage)
                        .cloned()
                        .ok_or_else(|| invalid("slice lineage pair missing"))?,
                ))
            })
            .collect()
    }
    fn value(
        &self,
        timing: &AudioTimingId,
        reference: &NodeId,
        value: &mut AudioRepeatValue,
    ) -> Result<(), EditError> {
        match value {
            AudioRepeatValue::Live { repeat } => *repeat = self.live(repeat)?,
            AudioRepeatValue::Captured { iteration } => {
                *iteration = self.play(
                    RepeatKey::Historical(timing.clone(), reference.clone()),
                    iteration,
                )?
            }
        }
        Ok(())
    }
    fn placement(&self, value: &mut AudioPlacementTemplate) -> Result<(), EditError> {
        let timing = value.reference.timing.clone();
        for argument in &mut value.arguments {
            self.value(&timing, &argument.reference_repeat, &mut argument.value)?;
            argument.reference_repeat = self.historical(&timing, &argument.reference_repeat)?;
        }
        if let Some(gap) = &mut value.gap_after {
            self.value(&timing, &value.reference.physical, gap)?;
        }
        for clause in &mut value.births {
            match &mut clause.survivors {
                AudioBirthSurvivors::CapturedRepeat { repeat } => {
                    *repeat = self.historical(&timing, repeat)?
                }
                AudioBirthSurvivors::Run {
                    allocation, first, ..
                } => {
                    let play = self.play(
                        RepeatKey::Live(clause.repeat.clone()),
                        &IterationId {
                            allocation: allocation.clone(),
                            ordinal: *first,
                        },
                    )?;
                    *allocation = play.allocation;
                    *first = play.ordinal;
                }
            }
            clause.repeat = self.live(&clause.repeat)?;
            clause.definition_root = self.historical(&timing, &clause.definition_root)?;
        }
        match &mut value.reference.root {
            AudioClockRoot::ProjectRootRoundEven => {}
            AudioClockRoot::PreserveInputPointCeil { stage } => {
                *stage = self.historical(&timing, stage)?
            }
            AudioClockRoot::DefinitionPointCeil { root } => {
                *root = self.historical(&timing, root)?
            }
            AudioClockRoot::GapDefinitionPointCeil { repeat } => {
                *repeat = self.historical(&timing, repeat)?
            }
        }
        value.reference.physical = self.historical(&timing, &value.reference.physical)?;
        value.reference.timing = self
            .timings
            .get(&timing)
            .cloned()
            .ok_or_else(|| invalid("slice timing missing"))?;
        Ok(())
    }
    fn bindings(
        &self,
        input: &BTreeMap<NodeId, OwnedAudioBinding>,
    ) -> Result<BTreeMap<NodeId, OwnedAudioBinding>, EditError> {
        input
            .iter()
            .map(|(owner, binding)| {
                let mut binding = binding.clone();
                for placement in binding.placements_mut() {
                    self.placement(placement)?;
                }
                Ok((self.live(owner)?, binding))
            })
            .collect()
    }
    fn layout(
        &self,
        timing: &AudioTimingId,
        layout: &FrozenAudioLayout,
    ) -> Result<FrozenAudioLayout, EditError> {
        let nodes = layout
            .nodes()
            .iter()
            .map(|(id, node)| {
                let mut node = node.clone();
                match &mut node.kind {
                    FrozenAudioKind::Sequence { children } => {
                        for child in children {
                            *child = self.historical(timing, child)?;
                        }
                    }
                    FrozenAudioKind::Repeat {
                        child, iterations, ..
                    } => {
                        *child = self.historical(timing, child)?;
                        *iterations = self.order(
                            RepeatKey::Historical(timing.clone(), id.clone()),
                            iterations,
                        )?;
                    }
                    FrozenAudioKind::Retime { child, .. } => {
                        *child = self.historical(timing, child)?
                    }
                    FrozenAudioKind::Source { .. } | FrozenAudioKind::Hold { .. } => {}
                }
                Ok((self.historical(timing, id)?, node))
            })
            .collect::<Result<_, EditError>>()?;
        Ok(FrozenAudioLayout::from_renamed_parts(
            self.historical(timing, layout.root())?,
            layout.rate(),
            nodes,
            self.overrides(layout.overrides(), Some(timing))?,
            self.overrides(layout.gap_overrides(), Some(timing))?,
            self.lineage(layout.audio_lineage(), Some(timing))?,
        )?)
    }
    fn mark(&self, mark: &Mark) -> Result<Mark, EditError> {
        let bindings = mark
            .bindings()
            .map(|mut binding| {
                binding.owner = self.live(&binding.owner)?;
                if binding.state == MarkState::Bound {
                    match &mut binding.coordinate {
                        Anchor::Local { node, .. } => *node = self.live(node)?,
                        Anchor::Occurrence { instance, .. } => {
                            instance.node = self.live(&instance.node)?;
                            for step in &mut instance.repeats {
                                step.iteration =
                                    self.play(RepeatKey::Live(step.node.clone()), &step.iteration)?;
                                step.node = self.live(&step.node)?;
                            }
                        }
                        Anchor::Source { .. } | Anchor::Sequence { .. } => {}
                    }
                }
                Ok(binding)
            })
            .collect::<Result<Vec<_>, EditError>>()?;
        mark.with_bindings(bindings)
            .ok_or_else(|| invalid("slice mark has no owned binding"))
    }
}

pub(super) fn validate_pools(
    document: &ProjectDocument,
    slice: &SliceWire,
    identities: &SlicePasteIdentities,
    requirements: SliceIdentityRequirements,
    split_nodes: &[NodeId],
    required_splits: usize,
) -> Result<(), EditError> {
    if identities.authored.nodes.len() < requirements.nodes
        || identities.authored.marks.len() < requirements.marks
        || identities.aliases.len() < requirements.aliases
        || split_nodes.len() < required_splits
    {
        return Err(limit("slice identity pool is insufficient"));
    }
    if identities.authored.nodes.len() > MAX_DOCUMENT_NODES
        || identities.authored.marks.len() > MAX_DOCUMENT_MARKS
        || identities.aliases.len() > MAX_AUDIO_BINDING_ENTRIES
        || split_nodes.len() > MAX_DOCUMENT_NODES
        || document
            .nodes
            .len()
            .checked_add(requirements.nodes)
            .and_then(|count| count.checked_add(required_splits))
            .is_none_or(|count| count > MAX_DOCUMENT_NODES)
    {
        return Err(limit("slice identities exceed destination limits"));
    }
    let mut occupied: BTreeSet<_> = document.nodes.keys().collect();
    occupied.extend(slice.nodes.keys());
    for layout in slice.audio_bindings.timings.values() {
        occupied.extend(layout.nodes().keys());
        occupied.extend(
            layout
                .audio_lineage()
                .values()
                .map(|lineage| &lineage.origin),
        );
    }
    occupied.extend(slice.audio_lineage.values().map(|lineage| &lineage.origin));
    for layout in document.audio_bindings.timings.values() {
        occupied.extend(layout.nodes().keys());
        occupied.extend(
            layout
                .audio_lineage()
                .values()
                .map(|lineage| &lineage.origin),
        );
    }
    occupied.extend(
        document
            .audio_lineage
            .values()
            .map(|lineage| &lineage.origin),
    );
    for id in identities
        .authored
        .nodes
        .iter()
        .chain(&identities.aliases)
        .chain(split_nodes)
    {
        if !occupied.insert(id) {
            return Err(EditError::new(
                EditErrorCode::IdentityConflict,
                "slice authored and historical identities must be fresh and distinct",
            ));
        }
    }
    let mut marks: BTreeSet<_> = document.marks.keys().collect();
    marks.extend(slice.marks.keys());
    for id in &identities.authored.marks {
        if !marks.insert(id) {
            return Err(EditError::new(
                EditErrorCode::IdentityConflict,
                "slice mark identities must be fresh and distinct",
            ));
        }
    }
    Ok(())
}

pub(super) struct Imported {
    nodes: BTreeMap<NodeId, BeatNode>,
    assets: BTreeMap<AssetId, AssetRecord>,
    marks: BTreeMap<MarkId, Mark>,
    overrides: BTreeMap<NodeId, PlayOverrides>,
    gap_overrides: BTreeMap<NodeId, PlayOverrides>,
    lineage: BTreeMap<NodeId, AudioLineageId>,
    bindings: AudioBindingState,
    beat_sounds: BTreeMap<NodeId, BTreeMap<SoundId, BeatSound>>,
    targets: BTreeMap<crate::TargetId, crate::AttentionTarget>,
}
impl Imported {
    pub(super) fn install(self, document: &mut ProjectDocument) -> Result<(), EditError> {
        let mut bindings = document.audio_bindings.clone();
        bindings.timings.extend(self.bindings.timings);
        bindings.bindings.extend(self.bindings.bindings);
        bindings.gap_bindings.extend(self.bindings.gap_bindings);
        bindings.sound_clocks.extend(self.bindings.sound_clocks);
        // Validate aggregate historical work and serialized bounds before graph changes.
        bindings.to_json()?;
        document.audio_bindings = bindings;
        document.nodes.extend(self.nodes);
        document.beat_sounds.extend(self.beat_sounds);
        document.assets.extend(self.assets);
        // The destination's current target wins over a historical copy.
        for (id, target) in self.targets {
            document.targets.entry(id).or_insert(target);
        }
        document.marks.extend(self.marks);
        document.overrides.extend(self.overrides);
        document.gap_overrides.extend(self.gap_overrides);
        document.audio_lineage.extend(self.lineage);
        Ok(())
    }
}

pub(super) fn prepare(
    slice: &SliceWire,
    identities: &SlicePasteIdentities,
    timing: &AudioTimingId,
) -> Result<Imported, EditError> {
    let inventory = Inventory::new(slice)?;
    let ranges = inventory.compact_ranges()?;
    let node_ids: BTreeMap<_, _> = slice
        .nodes
        .keys()
        .cloned()
        .zip(identities.authored.nodes.iter().skip(1).cloned())
        .collect();
    let mut alias_pool = identities.aliases.iter();
    let aliases = inventory
        .aliases
        .iter()
        .map(|key| {
            Ok((
                key.clone(),
                alias_pool
                    .next()
                    .ok_or_else(|| limit("slice alias pool exhausted"))?
                    .clone(),
            ))
        })
        .collect::<Result<_, EditError>>()?;
    let lineages = inventory
        .lineages
        .iter()
        .map(|key| {
            Ok((
                key.clone(),
                AudioLineageId {
                    allocation: timing.allocation.clone(),
                    origin: alias_pool
                        .next()
                        .ok_or_else(|| limit("slice lineage pool exhausted"))?
                        .clone(),
                },
            ))
        })
        .collect::<Result<_, EditError>>()?;
    let timings = slice
        .audio_bindings
        .timings
        .keys()
        .enumerate()
        .map(|(index, old)| {
            let offset = u32::try_from(index).map_err(|_| limit("slice timing offset overflow"))?;
            Ok((
                old.clone(),
                AudioTimingId {
                    allocation: timing.allocation.clone(),
                    ordinal: timing
                        .ordinal
                        .checked_add(offset)
                        .ok_or_else(|| limit("slice timing ordinal overflow"))?,
                },
            ))
        })
        .collect::<Result<_, EditError>>()?;
    let rename = Renamer {
        inventory,
        ranges,
        nodes: node_ids.clone(),
        aliases,
        lineages,
        timings,
        allocation: timing.allocation.clone(),
    };
    let mut nodes = BTreeMap::new();
    for (id, node) in &slice.nodes {
        let mut node = node.clone();
        match &mut node.kind {
            NodeKind::Sequence { children } => {
                for child in children {
                    *child = rename.live(child)?;
                }
            }
            NodeKind::Repeat {
                child, iterations, ..
            } => {
                *child = rename.live(child)?;
                *iterations = rename.order(RepeatKey::Live(id.clone()), iterations)?;
            }
            NodeKind::Retime { child, .. } => *child = rename.live(child)?,
            NodeKind::Source { .. } | NodeKind::Hold { .. } => {}
        }
        nodes.insert(rename.live(id)?, node);
    }
    let mut context = ProjectDocument::new(
        slice.project_id.clone(),
        slice.revision_id.clone(),
        slice.presentation_basis.clone(),
        slice.parent.clone(),
    )?;
    context.nodes.extend(slice.nodes.clone());
    context.assets = slice.assets.clone();
    context.targets = slice.targets.clone();
    context.overrides = slice.overrides.clone();
    context.gap_overrides = slice.gap_overrides.clone();
    context.nodes.insert(
        slice.parent.clone(),
        BeatNode::sequence(
            "Captured contents",
            slice.parts.iter().map(|part| part.root.clone()).collect(),
        ),
    );
    let durations = context.durations()?;
    let mut wrappers = identities.authored.nodes.iter().skip(slice.nodes.len() + 1);
    let mut children = Vec::with_capacity(slice.parts.len());
    for part in &slice.parts {
        let child = rename.live(&part.root)?;
        if part.mapping.duration() == durations[&part.root] {
            children.push(child);
        } else {
            let wrapper = wrappers
                .next()
                .ok_or_else(|| limit("slice window identity pool exhausted"))?
                .clone();
            nodes.insert(
                wrapper.clone(),
                BeatNode {
                    label: "Copied interval".into(),
                    framing: None,
                    audio_treatments: Default::default(),
                    audio_editorial_edges: Default::default(),
                    audio_edges: Default::default(),
                    kind: NodeKind::Retime {
                        child,
                        duration: part.mapping.duration(),
                        mapping: part.mapping,
                        pitch: PitchPolicy::Preserve,
                        purpose: RetimePurpose::Partition,
                    },
                    cutaways: Vec::new(),
                },
            );
            children.push(wrapper);
        }
    }
    nodes.insert(
        identities.authored.nodes[0].clone(),
        BeatNode::sequence("Copied contents", children),
    );
    let marks = slice
        .marks
        .values()
        .zip(&identities.authored.marks)
        .map(|(mark, id)| Ok((id.clone(), rename.mark(mark)?)))
        .collect::<Result<_, EditError>>()?;
    let timings = slice
        .audio_bindings
        .timings
        .iter()
        .map(|(id, layout)| Ok((rename.timings[id].clone(), rename.layout(id, layout)?)))
        .collect::<Result<_, EditError>>()?;
    let beat_sounds = slice
        .beat_sounds
        .iter()
        .map(|(owner, events)| {
            let new_owner = node_ids
                .get(owner)
                .cloned()
                .ok_or_else(|| invalid("beat sound owner is outside the captured slice"))?;
            Ok((new_owner, events.clone()))
        })
        .collect::<Result<_, EditError>>()?;
    let sound_clocks = slice
        .audio_bindings
        .sound_clocks
        .iter()
        .map(|(owner, events)| {
            let events = events
                .iter()
                .map(|(sound, journal)| {
                    let clocks = journal
                        .clocks()
                        .iter()
                        .map(|reference| {
                            Ok(SoundClockReference::new(
                                rename.timings[reference.timing()].clone(),
                                rename.historical(reference.timing(), reference.scope())?,
                                rename.historical(reference.timing(), reference.owner())?,
                            ))
                        })
                        .collect::<Result<_, EditError>>()?;
                    Ok((
                        sound.clone(),
                        SoundClockJournal::new(rename.live(journal.scope())?, clocks)?,
                    ))
                })
                .collect::<Result<_, EditError>>()?;
            Ok((rename.live(owner)?, events))
        })
        .collect::<Result<_, EditError>>()?;
    Ok(Imported {
        targets: slice.targets.clone(),
        nodes,
        assets: slice.assets.clone(),
        marks,
        overrides: rename.overrides(&slice.overrides, None)?,
        gap_overrides: rename.overrides(&slice.gap_overrides, None)?,
        lineage: rename.lineage(&slice.audio_lineage, None)?,
        bindings: AudioBindingState {
            timings,
            bindings: rename.bindings(&slice.audio_bindings.bindings)?,
            gap_bindings: rename.bindings(&slice.audio_bindings.gap_bindings)?,
            sound_clocks,
        },
        beat_sounds,
    })
}
