//! Compact, variable-duration Repeat evaluation shared by anchors and plans.
//! An override owns an ordinary editable subtree for one stable play identity.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    DocumentError, DocumentErrorCode, ExactRatio, InsertionBias, IterationId, IterationOrder,
    MAX_DOCUMENT_NODES, NodeId, RevisionId, TimeError,
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

mod clock;
use clock::Clock;
mod api;
pub use api::{
    ExactRepeatLayout, ExactRepeatLocation, ExactRepeatPlay, RepeatLayout, RepeatLocation,
    RepeatPlay,
};

#[derive(Debug, Clone)]
struct Segment<C: Clock> {
    start: C::Position,
    end: C::Position,
    first_play: u32,
    count: u32,
    allocation: RevisionId,
    first: u32,
    child: NodeId,
    child_duration: C::Duration,
    gap_child: Option<NodeId>,
    gap_duration: C::Duration,
}

#[derive(Debug, Clone)]
struct Layout<C: Clock> {
    segments: Vec<Segment<C>>,
    plays: u32,
    duration: C::Duration,
}

#[derive(Debug, Clone)]
struct Play<C: Clock> {
    index: u32,
    iteration: IterationId,
    child: NodeId,
    start: C::Position,
    duration: C::Duration,
    gap_after: C::Duration,
    gap_child: Option<NodeId>,
}

#[derive(Debug, Clone)]
struct Location<C: Clock> {
    play: Play<C>,
    segment_end: C::Position,
    position: ExactRatio,
    in_gap: bool,
    comparisons: usize,
}

impl<C: Clock> Layout<C> {
    /// Compile sparse play and gap branches together. A gap override is keyed
    /// by its preceding stable play, including when that play is currently last.
    /// Only the final gap's output is suppressed; its owned branch is validated.
    fn compile(
        iterations: &IterationOrder,
        child: &NodeId,
        overrides: Option<&PlayOverrides>,
        gap: C::Duration,
        gap_overrides: Option<&PlayOverrides>,
        durations: &BTreeMap<NodeId, C::Duration>,
    ) -> Result<Self, DocumentError> {
        iterations.validate()?;
        validate_duration::<C>(gap, false)?;
        let child_duration = duration::<C>(durations, child)?;
        let mut layout = Self {
            segments: Vec::new(),
            plays: iterations.len(),
            duration: C::ZERO,
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
                            duration::<C>(durations, effective_child)?
                        } else {
                            child_duration
                        },
                    ),
                    (
                        gap_root,
                        if let Some(root) = gap_root {
                            gap_duration::<C>(durations, root)?
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
        play: (&NodeId, C::Duration),
        gap: (Option<&NodeId>, C::Duration),
    ) -> Result<(), DocumentError> {
        if count == 0 {
            return Ok(());
        }
        let (child, child_duration) = play;
        let (gap_child, gap_duration) = gap;
        let start = C::start(self.duration);
        let gaps = count - u32::from(first_play + count == self.plays);
        let end = C::prefix(start, child_duration, gap_duration, count, gaps)?;
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
        self.duration = C::duration(end)?;
        Ok(())
    }

    pub fn duration(&self) -> C::Duration {
        self.duration
    }
    pub fn segment_count(&self) -> usize {
        self.segments.len()
    }

    /// Identity lookup scans compact segments, never individual plays.
    fn play(&self, identity: &IterationId) -> Result<Option<Play<C>>, DocumentError> {
        self.segments
            .iter()
            .find_map(|segment| {
                if segment.allocation != identity.allocation
                    || identity.ordinal < segment.first
                    || u64::from(identity.ordinal)
                        >= u64::from(segment.first) + u64::from(segment.count)
                {
                    return None;
                }
                Some(self.in_segment(segment, identity.ordinal - segment.first))
            })
            .transpose()
    }

    fn in_segment(&self, segment: &Segment<C>, offset: u32) -> Result<Play<C>, DocumentError> {
        let index = segment.first_play + offset;
        let start = C::prefix(
            segment.start,
            segment.child_duration,
            segment.gap_duration,
            offset,
            offset,
        )?;
        Ok(Play {
            index,
            iteration: IterationId {
                allocation: segment.allocation.clone(),
                ordinal: segment.first + offset,
            },
            child: segment.child.clone(),
            start,
            duration: segment.child_duration,
            gap_after: if index + 1 == self.plays {
                C::ZERO
            } else {
                segment.gap_duration
            },
            gap_child: segment.gap_child.clone(),
        })
    }

    /// Stop before exceeding the permitted number of segment comparisons.
    /// `LimitExceeded` is returned before evaluating an additional comparison;
    /// successful results report the actual count for a caller's shared budget.
    pub fn locate_bounded(
        &self,
        position: ExactRatio,
        bias: InsertionBias,
        maximum_comparisons: usize,
    ) -> Result<Location<C>, DocumentError> {
        if position.compare_integer(0).is_lt()
            || C::compare(position, C::start(self.duration)).is_gt()
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
            let comparison = C::compare(position, self.segments[middle].end);
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
        // A single play needs no period. In particular, a dormant final gap
        // must not overflow arithmetic for content that never renders it.
        let ordinal = if segment.count == 1 {
            0
        } else {
            let relative = position.checked_sub(C::coordinate(segment.start))?;
            let period = C::period(segment.child_duration, segment.gap_duration)?;
            let quotient = relative.checked_div(period)?;
            let mut ordinal = quotient.floor();
            if bias == InsertionBias::Left && ordinal > 0 && quotient.denominator() == 1 {
                ordinal -= 1;
            }
            u32::try_from(ordinal).map_err(|_| TimeError::Overflow)?
        };
        if ordinal >= segment.count {
            return Err(invalid("Repeat boundary has no play"));
        }
        let play = self.in_segment(segment, ordinal)?;
        let mut position = position.checked_sub(C::coordinate(play.start))?;
        let in_gap = C::compare(position, C::start(play.duration)).is_gt()
            || (C::compare(position, C::start(play.duration)).is_eq()
                && bias == InsertionBias::Right
                && C::length(play.gap_after) != ExactRatio::ZERO);
        if in_gap {
            position = position.checked_sub(C::length(play.duration))?;
        }
        Ok(Location {
            play,
            segment_end: segment.end,
            position,
            in_gap,
            comparisons,
        })
    }
}

fn duration<C: Clock>(
    durations: &BTreeMap<NodeId, C::Duration>,
    node: &NodeId,
) -> Result<C::Duration, DocumentError> {
    let duration = *durations
        .get(node)
        .ok_or_else(|| invalid("override child has no evaluated duration"))?;
    validate_duration::<C>(duration, true)?;
    Ok(duration)
}
fn gap_duration<C: Clock>(
    durations: &BTreeMap<NodeId, C::Duration>,
    node: &NodeId,
) -> Result<C::Duration, DocumentError> {
    let duration = durations
        .get(node)
        .copied()
        .ok_or_else(|| invalid("gap override root has no evaluated duration"))?;
    validate_duration::<C>(duration, false)?;
    Ok(duration)
}

fn validate_duration<C: Clock>(duration: C::Duration, positive: bool) -> Result<(), DocumentError> {
    let length = C::length(duration);
    if length.compare_integer(0).is_lt() {
        return Err(TimeError::NegativeDuration.into());
    }
    if positive && length == ExactRatio::ZERO {
        return Err(TimeError::EmptyRepeatChild.into());
    }
    Ok(())
}

fn invalid(message: &str) -> DocumentError {
    DocumentError::new(DocumentErrorCode::InvalidIdentity, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FrameDuration;

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
