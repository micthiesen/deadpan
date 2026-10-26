//! Explicit sparse Repeat branch copies inside a retained processing scope.

use super::*;

pub(super) struct IsolationProofState<'a> {
    branches: BTreeMap<(&'a NodeId, &'a crate::IterationId), &'a crate::SoundClockIsolation>,
    visited: BTreeSet<(NodeId, crate::IterationId)>,
    pub aliases: BTreeMap<NodeId, NodeId>,
}

impl<'a> IsolationProofState<'a> {
    pub fn new(
        relations: &'a [crate::SoundClockIsolation],
        work: &mut usize,
        maximum: usize,
    ) -> Result<Self, DocumentError> {
        spend(work, relations.len(), maximum)?;
        let mut branches = BTreeMap::new();
        let mut roots = BTreeSet::new();
        for relation in relations {
            if branches
                .insert((&relation.live_repeat, &relation.iteration), relation)
                .is_some()
                || !roots.insert(&relation.live_root)
                || relation.live_root == relation.live_repeat
            {
                return Err(invalid("invalid sound clock isolation relation"));
            }
        }
        Ok(Self {
            branches,
            visited: BTreeSet::new(),
            aliases: BTreeMap::new(),
        })
    }

    pub fn complete(&self) -> bool {
        self.visited.len() == self.branches.len()
    }

    pub fn children(
        &mut self,
        historical: &FrozenAudioLayout,
        current: &FrozenAudioLayout,
        old_id: &NodeId,
        live_id: &NodeId,
        work: &mut usize,
        maximum: usize,
    ) -> Result<Vec<(NodeId, NodeId)>, DocumentError> {
        let (
            FrozenAudioKind::Repeat {
                child: old_child,
                iterations: old_order,
                ..
            },
            FrozenAudioKind::Repeat {
                child: live_child,
                iterations: live_order,
                ..
            },
        ) = (&historical.nodes[old_id].kind, &current.nodes[live_id].kind)
        else {
            return paired_sound_clock_children(historical, current, old_id, live_id);
        };
        if old_order != live_order
            || !same_override_keys(
                historical.gap_overrides.get(old_id),
                current.gap_overrides.get(live_id),
            )
        {
            return Err(invalid("sound clock Repeat override keys changed"));
        }
        let old = historical.overrides.get(old_id);
        let live = current.overrides.get(live_id);
        if old.is_some_and(|old| {
            old.iter()
                .any(|(iteration, _)| live.and_then(|live| live.get(iteration)).is_none())
        }) {
            return Err(invalid(
                "sound clock isolation removed an existing override",
            ));
        }
        let mut children = vec![(old_child.clone(), live_child.clone())];
        for (iteration, root) in live.into_iter().flat_map(PlayOverrides::iter) {
            spend(work, 1, maximum)?;
            if let Some(old_root) = old.and_then(|old| old.get(iteration)) {
                children.push((old_root.clone(), root.clone()));
                continue;
            }
            let relation = self
                .branches
                .get(&(live_id, iteration))
                .ok_or_else(|| invalid("sound clock Repeat override lacks isolation proof"))?;
            if relation.historical_repeat != *old_id
                || relation.live_root != *root
                || old_order.position(iteration).is_none()
                || !self.visited.insert((live_id.clone(), iteration.clone()))
                || self
                    .aliases
                    .insert(root.clone(), old_child.clone())
                    .is_some()
            {
                return Err(invalid("sound clock isolation names another Repeat branch"));
            }
            // This alias grants only a route into the recursive verifier. The
            // entire cloned input, clocks and any recorded Split forests must
            // still pair with this effective historical default branch.
            children.push((old_child.clone(), root.clone()));
        }
        if let (Some(old), Some(live)) = (
            historical.gap_overrides.get(old_id),
            current.gap_overrides.get(live_id),
        ) {
            children.extend(
                old.iter()
                    .zip(live.iter())
                    .map(|((_, old), (_, live))| (old.clone(), live.clone())),
            );
        }
        Ok(children)
    }
}
