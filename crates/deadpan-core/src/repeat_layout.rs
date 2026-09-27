//! Compact, variable-duration Repeat evaluation shared by anchors and plans.
//! An override owns an ordinary editable subtree for one stable play identity.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    DocumentError, DocumentErrorCode, ExactRatio, FrameDuration, InsertionBias, IterationId,
    IterationOrder, MAX_DOCUMENT_NODES, NodeId, RevisionId, TimeError,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayOverride {
    pub iteration: IterationId,
    pub root: NodeId,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Vec<PlayOverride>", into = "Vec<PlayOverride>")]
pub struct PlayOverrides {
    entries: BTreeMap<IterationId, NodeId>,
}

impl TryFrom<Vec<PlayOverride>> for PlayOverrides {
    type Error = DocumentError;
    fn try_from(entries: Vec<PlayOverride>) -> Result<Self, Self::Error> {
        if entries.len() > MAX_DOCUMENT_NODES {
            return Err(invalid("too many sparse play overrides"));
        }
        let mut result = Self::default();
        for entry in entries {
            if result.entries.insert(entry.iteration, entry.root).is_some() {
                return Err(invalid("duplicate sparse play override identity"));
            }
        }
        Ok(result)
    }
}
impl From<PlayOverrides> for Vec<PlayOverride> {
    fn from(value: PlayOverrides) -> Self {
        value
            .entries
            .into_iter()
            .map(|(iteration, root)| PlayOverride { iteration, root })
            .collect()
    }
}
impl PlayOverrides {
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn get(&self, iteration: &IterationId) -> Option<&NodeId> {
        self.entries.get(iteration)
    }
    pub fn iter(
        &self,
    ) -> impl DoubleEndedIterator<Item = (&IterationId, &NodeId)> + ExactSizeIterator {
        self.entries.iter()
    }
    pub(crate) fn insert(&mut self, iteration: IterationId, root: NodeId) -> Option<NodeId> {
        self.entries.insert(iteration, root)
    }
    pub(crate) fn remove(&mut self, iteration: &IterationId) -> Option<NodeId> {
        self.entries.remove(iteration)
    }
}

#[derive(Debug, Clone)]
struct Segment {
    start: i64,
    end: i64,
    first_play: u32,
    count: u32,
    allocation: RevisionId,
    first: u32,
    child: NodeId,
    child_duration: FrameDuration,
    gap_child: Option<NodeId>,
    gap_duration: FrameDuration,
}

#[derive(Debug, Clone)]
pub struct RepeatLayout {
    segments: Vec<Segment>,
    plays: u32,
    duration: FrameDuration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepeatPlay {
    pub index: u32,
    pub iteration: IterationId,
    pub child: NodeId,
    pub start: i64,
    pub duration: FrameDuration,
    pub gap_after: FrameDuration,
    /// An explicit gap branch belongs to this stable play. It remains present
    /// when this is the final play, although that final gap renders no time.
    pub gap_child: Option<NodeId>,
}

impl RepeatPlay {
    /// The project-facing start of an active owned branch in this play.
    /// A final or zero-length gap still owns its `gap_child`, but has no
    /// project-resolvable occurrence.
    pub fn branch_offset(&self, child: &NodeId) -> Option<i64> {
        if &self.child == child {
            return Some(self.start);
        }
        if self.gap_after != FrameDuration::ZERO && self.gap_child.as_ref() == Some(child) {
            return self.start.checked_add(self.duration.frames());
        }
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepeatLocation {
    pub play: RepeatPlay,
    /// Child-local coordinate, or gap-local coordinate when `in_gap` is true.
    pub position: ExactRatio,
    pub in_gap: bool,
    pub comparisons: usize,
}

impl RepeatLayout {
    pub fn compile(
        iterations: &IterationOrder,
        child: &NodeId,
        overrides: Option<&PlayOverrides>,
        gap: FrameDuration,
        durations: &BTreeMap<NodeId, FrameDuration>,
    ) -> Result<Self, DocumentError> {
        Self::compile_with_gap_overrides(iterations, child, overrides, gap, None, durations)
    }

    /// Compile sparse play and gap branches together. A gap override is keyed
    /// by its preceding stable play, including when that play is currently last.
    /// Only the final gap's output is suppressed; its owned branch is validated.
    pub fn compile_with_gap_overrides(
        iterations: &IterationOrder,
        child: &NodeId,
        overrides: Option<&PlayOverrides>,
        gap: FrameDuration,
        gap_overrides: Option<&PlayOverrides>,
        durations: &BTreeMap<NodeId, FrameDuration>,
    ) -> Result<Self, DocumentError> {
        iterations.validate()?;
        let child_duration = duration(durations, child)?;
        let mut layout = Self {
            segments: Vec::new(),
            plays: iterations.len(),
            duration: FrameDuration::ZERO,
        };
        let mut play = 0u32;
        let mut found_plays = 0usize;
        let mut found_gaps = 0usize;
        for (allocation, first, count) in iterations.segments() {
            let end = u64::from(first) + u64::from(count);
            let mut cursor = u64::from(first);
            let lower = IterationId {
                allocation: allocation.clone(),
                ordinal: first,
            };
            let mut play_entries = overrides
                .into_iter()
                .flat_map(|entries| entries.entries.range(lower.clone()..))
                .peekable();
            let mut gap_entries = gap_overrides
                .into_iter()
                .flat_map(|entries| entries.entries.range(lower.clone()..))
                .peekable();
            while cursor < end {
                let next_play = play_entries.peek().and_then(|(identity, _)| {
                    (&identity.allocation == allocation && u64::from(identity.ordinal) < end)
                        .then_some(u64::from(identity.ordinal))
                });
                let next_gap = gap_entries.peek().and_then(|(identity, _)| {
                    (&identity.allocation == allocation && u64::from(identity.ordinal) < end)
                        .then_some(u64::from(identity.ordinal))
                });
                let next = next_play.into_iter().chain(next_gap).min().unwrap_or(end);
                let before = u32::try_from(next - cursor).map_err(|_| TimeError::Overflow)?;
                layout.append(
                    allocation,
                    u32::try_from(cursor).map_err(|_| TimeError::Overflow)?,
                    before,
                    play,
                    (child, child_duration),
                    (None, gap),
                )?;
                play += before;
                if next == end {
                    break;
                }
                let play_root = if next_play == Some(next) {
                    found_plays += 1;
                    Some(play_entries.next().expect("peeked play override").1)
                } else {
                    None
                };
                let gap_root = if next_gap == Some(next) {
                    found_gaps += 1;
                    Some(gap_entries.next().expect("peeked gap override").1)
                } else {
                    None
                };
                let effective_child = play_root.unwrap_or(child);
                layout.append(
                    allocation,
                    u32::try_from(next).map_err(|_| TimeError::Overflow)?,
                    1,
                    play,
                    (
                        effective_child,
                        if play_root.is_some() {
                            duration(durations, effective_child)?
                        } else {
                            child_duration
                        },
                    ),
                    (
                        gap_root,
                        if let Some(root) = gap_root {
                            gap_duration(durations, root)?
                        } else {
                            gap
                        },
                    ),
                )?;
                play += 1;
                cursor = next + 1;
            }
        }
        if found_plays != overrides.map_or(0, PlayOverrides::len) {
            return Err(invalid("override names a missing or retired Repeat play"));
        }
        if found_gaps != gap_overrides.map_or(0, PlayOverrides::len) {
            return Err(invalid(
                "gap override names a missing or retired Repeat play",
            ));
        }
        Ok(layout)
    }

    fn append(
        &mut self,
        allocation: &RevisionId,
        first: u32,
        count: u32,
        first_play: u32,
        play: (&NodeId, FrameDuration),
        gap: (Option<&NodeId>, FrameDuration),
    ) -> Result<(), DocumentError> {
        if count == 0 {
            return Ok(());
        }
        let (child, child_duration) = play;
        let (gap_child, gap_duration) = gap;
        let start = self.duration.frames();
        let period = i128::from(child_duration.frames()) + i128::from(gap_duration.frames());
        let trailing_gap = if first_play + count == self.plays {
            gap_duration.frames()
        } else {
            0
        };
        let end = i128::from(start) + period * i128::from(count) - i128::from(trailing_gap);
        let end = i64::try_from(end).map_err(|_| TimeError::Overflow)?;
        self.segments.push(Segment {
            start,
            end,
            first_play,
            count,
            allocation: allocation.clone(),
            first,
            child: child.clone(),
            child_duration,
            gap_child: gap_child.cloned(),
            gap_duration,
        });
        self.duration = FrameDuration::new(end)?;
        Ok(())
    }

    pub fn duration(&self) -> FrameDuration {
        self.duration
    }
    pub fn segment_count(&self) -> usize {
        self.segments.len()
    }

    /// Identity lookup scans compact segments, never individual plays.
    pub fn play(&self, identity: &IterationId) -> Option<RepeatPlay> {
        self.segments.iter().find_map(|segment| {
            if segment.allocation != identity.allocation
                || identity.ordinal < segment.first
                || u64::from(identity.ordinal)
                    >= u64::from(segment.first) + u64::from(segment.count)
            {
                return None;
            }
            Some(self.in_segment(segment, identity.ordinal - segment.first))
        })
    }

    fn in_segment(&self, segment: &Segment, offset: u32) -> RepeatPlay {
        let index = segment.first_play + offset;
        let period =
            i128::from(segment.child_duration.frames()) + i128::from(segment.gap_duration.frames());
        // Compilation checked every nonnegative prefix against i64.
        let start = i64::try_from(i128::from(segment.start) + i128::from(offset) * period)
            .expect("compiled Repeat prefixes fit i64");
        RepeatPlay {
            index,
            iteration: IterationId {
                allocation: segment.allocation.clone(),
                ordinal: segment.first + offset,
            },
            child: segment.child.clone(),
            start,
            duration: segment.child_duration,
            gap_after: if index + 1 == self.plays {
                FrameDuration::ZERO
            } else {
                segment.gap_duration
            },
            gap_child: segment.gap_child.clone(),
        }
    }

    /// Boundary bias selects the preceding/following content at an exact edge.
    /// The outside end with right bias has no content and is rejected.
    pub fn locate(
        &self,
        position: ExactRatio,
        bias: InsertionBias,
    ) -> Result<RepeatLocation, DocumentError> {
        self.locate_bounded(position, bias, usize::MAX)
    }

    /// Stop before exceeding the permitted number of segment comparisons.
    /// `LimitExceeded` is returned before evaluating an additional comparison;
    /// successful results report the actual count for a caller's shared budget.
    pub fn locate_bounded(
        &self,
        position: ExactRatio,
        bias: InsertionBias,
        maximum_comparisons: usize,
    ) -> Result<RepeatLocation, DocumentError> {
        if position.compare_integer(0).is_lt()
            || position.compare_integer(self.duration.frames()).is_gt()
        {
            return Err(invalid("position is outside Repeat duration"));
        }
        let mut comparisons = 0;
        let mut index = 0;
        let mut end = self.segments.len();
        while index < end {
            if comparisons == maximum_comparisons {
                return Err(DocumentError::new(
                    DocumentErrorCode::LimitExceeded,
                    "Repeat lookup comparison budget exhausted",
                ));
            }
            let middle = index + (end - index) / 2;
            comparisons += 1;
            let comparison = position.compare_integer(self.segments[middle].end);
            let preceding = match bias {
                InsertionBias::Left => comparison.is_gt(),
                InsertionBias::Right => !comparison.is_lt(),
            };
            if preceding {
                index = middle + 1;
            } else {
                end = middle;
            }
        }
        let segment = self
            .segments
            .get(index)
            .ok_or_else(|| invalid("Repeat boundary has no following content"))?;
        let relative = position.checked_sub(ExactRatio::integer(segment.start))?;
        let period =
            i128::from(segment.child_duration.frames()) + i128::from(segment.gap_duration.frames());
        let quotient = relative.checked_div(ExactRatio::new(period, 1)?)?;
        let mut ordinal = quotient.floor();
        if bias == InsertionBias::Left && ordinal > 0 && quotient.denominator() == 1 {
            ordinal -= 1;
        }
        let ordinal = u32::try_from(ordinal).map_err(|_| TimeError::Overflow)?;
        if ordinal >= segment.count {
            return Err(invalid("Repeat boundary has no play"));
        }
        let play = self.in_segment(segment, ordinal);
        let mut position = position.checked_sub(ExactRatio::integer(play.start))?;
        let in_gap = position.compare_integer(play.duration.frames()).is_gt()
            || (position.compare_integer(play.duration.frames()).is_eq()
                && bias == InsertionBias::Right
                && play.gap_after != FrameDuration::ZERO);
        if in_gap {
            position = position.checked_sub(ExactRatio::integer(play.duration.frames()))?;
        }
        Ok(RepeatLocation {
            play,
            position,
            in_gap,
            comparisons,
        })
    }
}

fn duration(
    durations: &BTreeMap<NodeId, FrameDuration>,
    node: &NodeId,
) -> Result<FrameDuration, DocumentError> {
    let duration = *durations
        .get(node)
        .ok_or_else(|| invalid("override child has no evaluated duration"))?;
    if duration == FrameDuration::ZERO {
        return Err(TimeError::EmptyRepeatChild.into());
    }
    Ok(duration)
}
fn gap_duration(
    durations: &BTreeMap<NodeId, FrameDuration>,
    node: &NodeId,
) -> Result<FrameDuration, DocumentError> {
    durations
        .get(node)
        .copied()
        .ok_or_else(|| invalid("gap override root has no evaluated duration"))
}
fn invalid(message: &str) -> DocumentError {
    DocumentError::new(DocumentErrorCode::InvalidIdentity, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_lookup_enforces_exact_comparison_allowance_on_sparse_runs() {
        let allocation = RevisionId::new("allocation").unwrap();
        let base = NodeId::new("base").unwrap();
        let alternate = NodeId::new("alternate").unwrap();
        let order = IterationOrder::new(allocation.clone(), 1_000_000_000).unwrap();
        let overrides = PlayOverrides::try_from(vec![PlayOverride {
            iteration: IterationId {
                allocation,
                ordinal: 500_000_000,
            },
            root: alternate.clone(),
        }])
        .unwrap();
        let durations = BTreeMap::from([
            (base.clone(), FrameDuration::new(1).unwrap()),
            (alternate, FrameDuration::new(3).unwrap()),
        ]);
        let layout = RepeatLayout::compile(
            &order,
            &base,
            Some(&overrides),
            FrameDuration::new(1).unwrap(),
            &durations,
        )
        .unwrap();
        assert_eq!(layout.segment_count(), 3);
        for position in [0, 1, 1_000_000_000, 1_000_000_003, 2_000_000_000] {
            let position = ExactRatio::integer(position);
            let found = layout.locate(position, InsertionBias::Right).unwrap();
            assert!(found.comparisons > 0);
            assert_eq!(
                layout
                    .locate_bounded(position, InsertionBias::Right, found.comparisons)
                    .unwrap(),
                found
            );
            assert_eq!(
                layout
                    .locate_bounded(position, InsertionBias::Right, found.comparisons - 1)
                    .unwrap_err()
                    .code,
                DocumentErrorCode::LimitExceeded
            );
            assert_eq!(
                layout
                    .locate_bounded(position, InsertionBias::Right, 0)
                    .unwrap_err()
                    .code,
                DocumentErrorCode::LimitExceeded
            );
        }
    }
}
