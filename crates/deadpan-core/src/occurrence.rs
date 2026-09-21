//! Stable, compact identity for repeated plays. Allocation is namespaced by the
//! never-reused revision that creates plays, so undo followed by a different edit
//! cannot accidentally revive an abandoned play identity.

use crate::{
    DocumentError, DocumentErrorCode, MAX_DOCUMENT_DEPTH, MAX_DOCUMENT_NODES, NodeId, RevisionId,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IterationId {
    pub allocation: RevisionId,
    pub ordinal: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct IterationRun {
    allocation: RevisionId,
    first: u32,
    count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "OrderWire")]
pub struct IterationOrder {
    runs: Vec<IterationRun>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OrderWire {
    runs: Vec<IterationRun>,
}
impl TryFrom<OrderWire> for IterationOrder {
    type Error = DocumentError;
    fn try_from(value: OrderWire) -> Result<Self, Self::Error> {
        let order = Self { runs: value.runs };
        order.validate()?;
        Ok(order)
    }
}

impl IterationOrder {
    pub fn new(allocation: RevisionId, plays: u32) -> Result<Self, DocumentError> {
        let order = Self {
            runs: vec![IterationRun {
                allocation,
                first: 0,
                count: plays,
            }],
        };
        order.validate()?;
        Ok(order)
    }
    pub fn len(&self) -> u32 {
        self.runs.iter().map(|run| run.count).sum()
    }
    pub fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }
    pub fn segment_count(&self) -> usize {
        self.runs.len()
    }
    /// Compact runs in play order, for indexed compilation without expanding plays.
    pub fn segments(&self) -> impl Iterator<Item = (&RevisionId, u32, u32)> {
        self.runs
            .iter()
            .map(|run| (&run.allocation, run.first, run.count))
    }
    pub fn at(&self, mut index: u32) -> Option<IterationId> {
        for run in &self.runs {
            if index < run.count {
                return Some(IterationId {
                    allocation: run.allocation.clone(),
                    ordinal: run.first + index,
                });
            }
            index -= run.count;
        }
        None
    }
    pub fn position(&self, identity: &IterationId) -> Option<u32> {
        let mut offset = 0_u32;
        for run in &self.runs {
            if run.allocation == identity.allocation
                && identity.ordinal >= run.first
                && u64::from(identity.ordinal) < u64::from(run.first) + u64::from(run.count)
            {
                return offset.checked_add(identity.ordinal - run.first);
            }
            offset += run.count;
        }
        None
    }
    pub fn validate(&self) -> Result<(), DocumentError> {
        if self.runs.is_empty() || self.runs.len() > MAX_DOCUMENT_NODES {
            return Err(invalid(
                "repeat identity order is empty or exceeds segment limit",
            ));
        }
        let mut total = 0u64;
        let mut allocations: BTreeMap<&RevisionId, Vec<(u64, u64)>> = BTreeMap::new();
        for run in &self.runs {
            let end = u64::from(run.first) + u64::from(run.count);
            total += u64::from(run.count);
            if run.count == 0 || end > u64::from(u32::MAX) + 1 || total > u64::from(u32::MAX) {
                return Err(invalid(
                    "repeat identity ranges are invalid or count overflows",
                ));
            }
            allocations
                .entry(&run.allocation)
                .or_default()
                .push((u64::from(run.first), end));
        }
        for intervals in allocations.values_mut() {
            intervals.sort_unstable();
            if intervals.windows(2).any(|pair| pair[0].1 > pair[1].0) {
                return Err(invalid("repeat contains duplicate iteration identities"));
            }
        }
        Ok(())
    }
    pub fn resized(&self, plays: u32, allocation: RevisionId) -> Result<Self, DocumentError> {
        if plays == 0 {
            return Err(invalid("repeat must have at least one play"));
        }
        let current = self.len();
        if plays > current {
            return self.inserted(current, plays - current, allocation);
        }
        let (runs, _) = self.split(plays)?;
        let order = Self { runs };
        order.validate()?;
        Ok(order)
    }
    pub fn inserted(
        &self,
        index: u32,
        count: u32,
        allocation: RevisionId,
    ) -> Result<Self, DocumentError> {
        if count == 0 || self.len().checked_add(count).is_none() {
            return Err(invalid("inserted play count is zero or overflows"));
        }
        let (mut runs, right) = self.split(index)?;
        runs.push(IterationRun {
            allocation,
            first: 0,
            count,
        });
        runs.extend(right);
        let mut order = Self { runs };
        order.coalesce();
        order.validate()?;
        Ok(order)
    }
    /// Destination is measured after removing the nonempty [start,end) range.
    pub fn moved(&self, start: u32, end: u32, destination: u32) -> Result<Self, DocumentError> {
        if start >= end || end > self.len() || destination > self.len() - (end - start) {
            return Err(invalid("iteration move range is outside the repeat"));
        }
        let (before, tail) = self.split(start)?;
        let (moving, after) = Self { runs: tail }.split(end - start)?;
        let remaining = Self {
            runs: before.into_iter().chain(after).collect(),
        };
        let (before, after) = remaining.split(destination)?;
        let mut order = Self {
            runs: before.into_iter().chain(moving).chain(after).collect(),
        };
        order.coalesce();
        order.validate()?;
        Ok(order)
    }
    fn split(&self, index: u32) -> Result<(Vec<IterationRun>, Vec<IterationRun>), DocumentError> {
        if index > self.len() {
            return Err(invalid("iteration position is outside the repeat"));
        }
        let mut left = Vec::new();
        let mut right = Vec::new();
        let mut remaining = index;
        for run in &self.runs {
            let take = remaining.min(run.count);
            if take > 0 {
                left.push(IterationRun {
                    count: take,
                    ..run.clone()
                });
            }
            if take < run.count {
                right.push(IterationRun {
                    first: run.first + take,
                    count: run.count - take,
                    ..run.clone()
                });
            }
            remaining -= take;
        }
        Ok((left, right))
    }
    fn coalesce(&mut self) {
        let mut compact: Vec<IterationRun> = Vec::with_capacity(self.runs.len());
        for run in self.runs.drain(..) {
            if let Some(last) = compact.last_mut()
                && last.allocation == run.allocation
                && u64::from(last.first) + u64::from(last.count) == u64::from(run.first)
                && let Some(count) = last.count.checked_add(run.count)
            {
                last.count = count;
                continue;
            }
            compact.push(run);
        }
        self.runs = compact;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepeatInstance {
    pub node: NodeId,
    pub iteration: IterationId,
}

/// Non-repeating grouping nodes do not enter the path: grouping cannot change
/// occurrence identity. The target and ordered repeated ancestors identify it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstancePath {
    pub node: NodeId,
    pub repeats: Vec<RepeatInstance>,
}

impl InstancePath {
    pub fn validate_depth(&self) -> Result<(), DocumentError> {
        if self.repeats.len() > MAX_DOCUMENT_DEPTH {
            return Err(invalid("instance path exceeds structural depth limit"));
        }
        Ok(())
    }
    pub fn validate(&self, document: &crate::ProjectDocument) -> Result<(), DocumentError> {
        self.validate_depth()?;
        if !document.nodes().contains_key(&self.node) {
            return Err(invalid("instance target does not exist"));
        }
        let parents: BTreeMap<_, _> = document
            .nodes()
            .keys()
            .flat_map(|id| document.children(id).map(move |child| (child, id)))
            .collect();
        let mut target = &self.node;
        let mut step = self.repeats.len();
        while let Some(parent) = parents.get(target) {
            if let crate::NodeKind::Repeat {
                iterations, child, ..
            } = &document.nodes()[*parent].kind
            {
                step = step
                    .checked_sub(1)
                    .ok_or_else(|| invalid("instance path omits a Repeat ancestor"))?;
                let instance = &self.repeats[step];
                let effective = document
                    .overrides()
                    .get(*parent)
                    .and_then(|entries| entries.get(&instance.iteration))
                    .unwrap_or(child);
                if &instance.node != *parent
                    || iterations.position(&instance.iteration).is_none()
                    || effective != target
                {
                    return Err(invalid(
                        "instance path names the wrong Repeat or a retired iteration",
                    ));
                }
            }
            target = parent;
        }
        if step != 0 {
            return Err(invalid("instance path contains extra Repeat ancestors"));
        }
        Ok(())
    }
}

fn invalid(message: &str) -> DocumentError {
    DocumentError::new(DocumentErrorCode::InvalidIdentity, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn revision(value: &str) -> RevisionId {
        RevisionId::new(value).unwrap()
    }

    proptest::proptest! {
        #[test]
        fn compact_moves_match_an_expanded_identity_model(count in 1u32..200, choices in proptest::collection::vec((0u32..200,0u32..200,0u32..200), 1..80)) {
            let mut order = IterationOrder::new(revision("initial"),count).unwrap();
            let mut expanded = (0..count).map(|position| order.at(position).unwrap()).collect::<Vec<_>>();
            for (a,b,c) in choices {
                let start = a % count;
                let end = start + 1 + b % (count-start);
                let destination = c % (count-(end-start)+1);
                order = order.moved(start,end,destination).unwrap();
                let moving = expanded.drain(start as usize..end as usize).collect::<Vec<_>>();
                expanded.splice(destination as usize..destination as usize,moving);
                for (position,identity) in expanded.iter().enumerate() {
                    proptest::prop_assert_eq!(order.at(position as u32),Some(identity.clone()));
                    proptest::prop_assert_eq!(order.position(identity),Some(position as u32));
                }
            }
        }
    }

    #[test]
    fn billions_of_plays_stay_compact_and_removed_ids_do_not_return() {
        let original = IterationOrder::new(revision("original"), u32::MAX).unwrap();
        assert_eq!(original.segment_count(), 1);
        let removed = original.at(2).unwrap();
        let smaller = original.resized(2, revision("shrink")).unwrap();
        let grown = smaller.resized(3, revision("new-branch")).unwrap();
        assert_eq!(grown.at(0), original.at(0));
        assert_eq!(grown.at(1), original.at(1));
        assert_eq!(grown.position(&removed), None);
        assert_eq!(grown.at(2).unwrap().allocation, revision("new-branch"));
        assert!(serde_json::to_string(&original).unwrap().len() < 128);
    }

    #[test]
    fn high_ordinals_after_small_prefix_do_not_overflow_position() {
        let original = IterationOrder::new(revision("large"), u32::MAX).unwrap();
        let last = original.at(u32::MAX - 1).unwrap();
        let high = original
            .moved(u32::MAX - 2, u32::MAX, 0)
            .unwrap()
            .resized(2, revision("shrink"))
            .unwrap();
        let inserted = high.inserted(0, 2, revision("prefix")).unwrap();
        assert_eq!(inserted.position(&last), Some(3));
        assert_eq!(inserted.at(3), Some(last));
    }

    #[test]
    fn reorder_and_insertion_preserve_identity_and_round_trip() {
        let original = IterationOrder::new(revision("create"), 5).unwrap();
        let moved = original.moved(1, 3, 3).unwrap();
        for (position, old) in [0, 3, 4, 1, 2].into_iter().enumerate() {
            assert_eq!(moved.at(position as u32), original.at(old));
        }
        let inserted = moved.inserted(2, 2, revision("insert")).unwrap();
        for old in 0..5 {
            let identity = original.at(old).unwrap();
            assert_eq!(
                inserted.at(inserted.position(&identity).unwrap()),
                Some(identity)
            );
        }
        assert_eq!(
            serde_json::from_str::<IterationOrder>(&serde_json::to_string(&inserted).unwrap())
                .unwrap(),
            inserted
        );
        assert!(original.inserted(0, 2, revision("create")).is_err());
        assert!(original.moved(0, 2, 4).is_err());
        assert!(original.resized(0, revision("empty")).is_err());
    }
}
