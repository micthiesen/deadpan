//! Exact, explicitly certified replacement of one clock leaf by Split pieces.

use super::*;

pub(super) struct SplitProofState<'a> {
    by_first: BTreeMap<NodeId, &'a crate::SoundClockSplit>,
    visited: BTreeSet<NodeId>,
    pub aliases: BTreeMap<NodeId, NodeId>,
    count: usize,
}

impl<'a> SplitProofState<'a> {
    pub fn new(
        splits: &'a [crate::SoundClockSplit],
        work: &mut usize,
        maximum: usize,
    ) -> Result<Self, DocumentError> {
        spend(work, splits.len(), maximum)?;
        let mut by_first = BTreeMap::new();
        let mut historical = BTreeSet::new();
        let mut live = BTreeSet::new();
        for split in splits {
            spend(work, split.pieces.len(), maximum)?;
            if split.pieces.len() < 2 || !historical.insert(split.historical_node.clone()) {
                return Err(invalid("invalid sound clock Split relation"));
            }
            for piece in &split.pieces {
                if !live.insert(piece.partition.clone()) || !live.insert(piece.full_owner.clone()) {
                    return Err(invalid("duplicate sound clock Split live identity"));
                }
            }
            by_first.insert(split.pieces[0].partition.clone(), split);
        }
        Ok(Self {
            by_first,
            visited: BTreeSet::new(),
            aliases: BTreeMap::new(),
            count: splits.len(),
        })
    }

    pub fn complete(&self) -> bool {
        self.visited.len() == self.count
    }

    pub fn root_forest(&self, historical: &NodeId, children: &[NodeId]) -> bool {
        children
            .first()
            .and_then(|first| self.by_first.get(first))
            .is_some_and(|split| {
                &split.historical_node == historical && split.pieces.len() == children.len()
            })
    }
}

pub(super) struct SequenceRelations<'a, 's> {
    pub historical: &'a FrozenAudioLayout,
    pub current: &'a FrozenAudioLayout,
    pub groups: &'a BTreeSet<NodeId>,
    pub visited_groups: &'a mut BTreeSet<NodeId>,
    pub live_parents: &'a mut BTreeMap<NodeId, NodeId>,
    pub historical_parents: &'a mut BTreeMap<NodeId, NodeId>,
    pub splits: &'a mut SplitProofState<'s>,
    pub work: &'a mut usize,
    pub maximum: usize,
}

impl SequenceRelations<'_, '_> {
    pub fn pair(
        &mut self,
        historical_parent: Option<&NodeId>,
        historical_children: &[NodeId],
        live: (&NodeId, &[NodeId], usize),
        root_wrapper: Option<&NodeId>,
    ) -> Result<Vec<SoundClockChildPair>, DocumentError> {
        let (live_parent, live_children, live_depth) = live;
        spend(self.work, live_children.len(), self.maximum)?;
        let mut pending: Vec<_> = live_children
            .iter()
            .enumerate()
            .rev()
            .map(|(slot, child)| (live_parent, child, live_depth + 1, slot))
            .collect();
        let mut flat = Vec::new();
        while let Some((parent, child, depth, slot)) = pending.pop() {
            spend(self.work, 1, self.maximum)?;
            if depth > MAX_DOCUMENT_DEPTH {
                return Err(limit("sound clock subtree depth exceeds limit"));
            }
            if !self.groups.contains(child) {
                flat.push((parent, child, depth, slot));
                continue;
            }
            let node = self
                .current
                .nodes
                .get(child)
                .ok_or_else(|| invalid("introduced sound clock group is missing"))?;
            let FrozenAudioKind::Sequence { children } = &node.kind else {
                return Err(invalid("introduced sound clock group is not a Sequence"));
            };
            spend(self.work, children.len() + 1, self.maximum)?;
            if self.current.derived_clocks.contains_key(child)
                || !self.visited_groups.insert(child.clone())
            {
                return Err(invalid("introduced sound clock group is not independent"));
            }
            if let Some(previous) = self.live_parents.insert(child.clone(), parent.clone())
                && (root_wrapper != Some(child) || previous != *parent)
            {
                return Err(invalid("introduced sound clock group has a shared edge"));
            }
            let mut duration = FrameDuration::ZERO;
            for grandchild in children {
                duration = duration.checked_add(
                    self.current
                        .nodes
                        .get(grandchild)
                        .ok_or_else(|| invalid("introduced sound clock group child is missing"))?
                        .duration,
                )?;
            }
            if duration != node.duration {
                return Err(invalid("introduced sound clock group changed its clock"));
            }
            pending.extend(
                children
                    .iter()
                    .enumerate()
                    .rev()
                    .map(|(slot, grandchild)| (child, grandchild, depth + 1, slot)),
            );
        }
        let mut pairs = Vec::new();
        let mut historical = historical_children.iter();
        let mut cursor = 0;
        while cursor < flat.len() {
            let (parent, child, depth, position) = flat[cursor];
            let historical_child = historical
                .next()
                .ok_or_else(|| invalid("sound clock processing subtree changed"))?;
            if let Some(split) = self.splits.by_first.get(child).copied() {
                if &split.historical_node != historical_child {
                    return Err(invalid("sound clock Split historical target changed"));
                }
                let end = cursor
                    .checked_add(split.pieces.len())
                    .filter(|end| *end <= flat.len())
                    .ok_or_else(|| invalid("sound clock Split forest is incomplete"))?;
                spend(self.work, split.pieces.len(), self.maximum)?;
                if !flat[cursor..end]
                    .iter()
                    .zip(&split.pieces)
                    .all(|((p, node, _, _), piece)| *p == parent && *node == &piece.partition)
                {
                    return Err(invalid(
                        "sound clock Split pieces are not an ordered adjacent forest",
                    ));
                }
                let FrozenAudioKind::Sequence { children } = &self.current.nodes[parent].kind
                else {
                    return Err(invalid("sound clock Split forest parent is not a Sequence"));
                };
                let sequence_end = position
                    .checked_add(split.pieces.len())
                    .ok_or_else(|| limit("sound clock Split forest index overflow"))?;
                if children.get(position..sequence_end).is_none_or(|children| {
                    !children
                        .iter()
                        .eq(split.pieces.iter().map(|piece| &piece.partition))
                }) {
                    return Err(invalid("sound clock Split pieces are not adjacent"));
                }
                let old = &self.historical.nodes[historical_child];
                let (full, range) = match &old.kind {
                    FrozenAudioKind::Source { .. } | FrozenAudioKind::Hold { .. } => (
                        historical_child,
                        FrameRange::new(
                            crate::ProjectFrame(0),
                            crate::ProjectFrame(old.duration.frames()),
                        )?,
                    ),
                    FrozenAudioKind::Retime {
                        child,
                        mapping,
                        pitch,
                        purpose: RetimePurpose::Partition,
                    } if old.duration == mapping.duration()
                        && !pitch.processes(true)
                        && !self
                            .historical
                            .derived_clocks
                            .contains_key(historical_child)
                        && matches!(
                            self.historical.nodes[child].kind,
                            FrozenAudioKind::Source { .. } | FrozenAudioKind::Hold { .. }
                        ) =>
                    {
                        (child, *mapping)
                    }
                    _ => {
                        return Err(invalid(
                            "sound clock Split target is not a complete leaf or transparent Partition",
                        ));
                    }
                };
                if self.historical.derived_clocks.contains_key(full) {
                    return Err(invalid("sound clock Split full owner has a derived clock"));
                }
                if full != historical_child
                    && let Some(historical_parent) = historical_parent
                {
                    self.historical_parents
                        .insert(historical_child.clone(), historical_parent.clone());
                }
                let mut next = range.start();
                for ((_, _, piece_depth, _), piece) in flat[cursor..end].iter().zip(&split.pieces) {
                    spend(self.work, 3, self.maximum)?;
                    if self.historical.nodes.contains_key(&piece.partition)
                        && !(&piece.partition == historical_child
                            && matches!(
                                old.kind,
                                FrozenAudioKind::Retime {
                                    purpose: RetimePurpose::Partition,
                                    ..
                                }
                            ))
                    {
                        return Err(invalid(
                            "sound clock Split Partition is not a fresh delivery identity",
                        ));
                    }
                    let node = self
                        .current
                        .nodes
                        .get(&piece.partition)
                        .ok_or_else(|| invalid("sound clock Split Partition is missing"))?;
                    let FrozenAudioKind::Retime {
                        child,
                        mapping,
                        pitch,
                        purpose: RetimePurpose::Partition,
                    } = &node.kind
                    else {
                        return Err(invalid("sound clock Split piece is not a Partition"));
                    };
                    if child != &piece.full_owner
                        || mapping.start() != next
                        || node.duration != mapping.duration()
                        || pitch.processes(true)
                        || self.current.derived_clocks.contains_key(&piece.partition)
                        || self.current.derived_clocks.contains_key(child)
                        || self.current.nodes.get(child) != self.historical.nodes.get(full)
                        || self
                            .current
                            .index
                            .parents
                            .get(child)
                            .map(|(parent, _)| parent)
                            != Some(&piece.partition)
                    {
                        return Err(invalid(
                            "sound clock Split changes its complete owner clock",
                        ));
                    }
                    next = mapping.end();
                    if next > range.end() {
                        return Err(invalid("sound clock Split exceeds its historical interval"));
                    }
                    if self
                        .live_parents
                        .insert(piece.partition.clone(), parent.clone())
                        .is_some()
                        || self
                            .splits
                            .aliases
                            .insert(piece.full_owner.clone(), full.clone())
                            .is_some()
                    {
                        return Err(invalid("sound clock Split contains a shared live child"));
                    }
                    pairs.push(SoundClockChildPair {
                        historical: full.clone(),
                        live: child.clone(),
                        live_parent: piece.partition.clone(),
                        historical_parent: (full != historical_child)
                            .then(|| historical_child.clone()),
                        live_depth: piece_depth + 1,
                    });
                }
                if next != range.end() || !self.splits.visited.insert(historical_child.clone()) {
                    return Err(invalid(
                        "sound clock Split does not cover its historical interval",
                    ));
                }
                cursor = end;
            } else {
                pairs.push(SoundClockChildPair {
                    historical: historical_child.clone(),
                    live: child.clone(),
                    live_parent: parent.clone(),
                    historical_parent: None,
                    live_depth: depth,
                });
                cursor += 1;
            }
        }
        if historical.next().is_some() {
            return Err(invalid("sound clock processing subtree changed"));
        }
        Ok(pairs)
    }
}
