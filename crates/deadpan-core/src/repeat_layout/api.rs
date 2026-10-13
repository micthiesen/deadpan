use std::collections::BTreeMap;

use crate::{
    DocumentError, ExactRatio, FrameDuration, InsertionBias, IterationId, IterationOrder, NodeId,
};

use super::{
    Layout, Location, Play, PlayOverrides,
    clock::{ExactClock, IntegerClock},
};

/// Compact Repeat prefixes in the existing authored integer-frame clock.
#[derive(Debug, Clone)]
pub struct RepeatLayout(Layout<IntegerClock>);

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
    /// The start of an active owned branch in this Repeat's clock. A final or
    /// zero-length gap retains its owner but has no resolvable occurrence.
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
    /// Exclusive end of the compact run, allowing irrelevant plays to be
    /// skipped without expanding them.
    pub segment_end: i64,
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

    /// Sparse play and gap branches share stable iteration identities. The
    /// final gap is validated and retained, but contributes no rendered time.
    pub fn compile_with_gap_overrides(
        iterations: &IterationOrder,
        child: &NodeId,
        overrides: Option<&PlayOverrides>,
        gap: FrameDuration,
        gap_overrides: Option<&PlayOverrides>,
        durations: &BTreeMap<NodeId, FrameDuration>,
    ) -> Result<Self, DocumentError> {
        Layout::compile(iterations, child, overrides, gap, gap_overrides, durations).map(Self)
    }

    pub fn duration(&self) -> FrameDuration {
        self.0.duration()
    }
    pub fn segment_count(&self) -> usize {
        self.0.segment_count()
    }
    pub fn play(&self, identity: &IterationId) -> Option<RepeatPlay> {
        // Every integer prefix is nonnegative and bounded by the compiled
        // total. The widened integer kernel cannot overflow before that check.
        self.0
            .play(identity)
            .expect("compiled Repeat prefixes fit i64")
            .map(Into::into)
    }
    /// Left/right bias chooses the preceding/following content at a seam.
    /// The outside end with right bias has no content and is rejected.
    pub fn locate(
        &self,
        position: ExactRatio,
        bias: InsertionBias,
    ) -> Result<RepeatLocation, DocumentError> {
        self.locate_bounded(position, bias, usize::MAX)
    }
    /// Fail before a segment comparison exceeds the shared caller budget.
    pub fn locate_bounded(
        &self,
        position: ExactRatio,
        bias: InsertionBias,
        maximum_comparisons: usize,
    ) -> Result<RepeatLocation, DocumentError> {
        self.0
            .locate_bounded(position, bias, maximum_comparisons)
            .map(Into::into)
    }
}

impl From<Play<IntegerClock>> for RepeatPlay {
    fn from(play: Play<IntegerClock>) -> Self {
        Self {
            index: play.index,
            iteration: play.iteration,
            child: play.child,
            start: play.start,
            duration: play.duration,
            gap_after: play.gap_after,
            gap_child: play.gap_child,
        }
    }
}
impl From<Location<IntegerClock>> for RepeatLocation {
    fn from(location: Location<IntegerClock>) -> Self {
        Self {
            play: location.play.into(),
            segment_end: location.segment_end,
            position: location.position,
            in_gap: location.in_gap,
            comparisons: location.comparisons,
        }
    }
}

/// Compact Repeat prefixes for derived, possibly fractional owner clocks.
///
/// This is a numerical index, not an authored duration or a document format.
/// Inputs must be exact nonnegative extents; every play must be positive.
/// No query rounds coordinates. Checked arithmetic can refuse an unrepresentable
/// prefix or local coordinate, including during lookup, rather than panic or
/// select neighboring content. Storage depends on runs and sparse overrides,
/// never on the number of rendered plays.
#[derive(Debug, Clone)]
pub struct ExactRepeatLayout(Layout<ExactClock>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExactRepeatPlay {
    pub index: u32,
    pub iteration: IterationId,
    pub child: NodeId,
    pub start: ExactRatio,
    pub duration: ExactRatio,
    pub gap_after: ExactRatio,
    /// Retained even when the final gap is dormant.
    pub gap_child: Option<NodeId>,
}

impl ExactRepeatPlay {
    pub fn branch_offset(&self, child: &NodeId) -> Result<Option<ExactRatio>, DocumentError> {
        if &self.child == child {
            return Ok(Some(self.start));
        }
        if self.gap_after != ExactRatio::ZERO && self.gap_child.as_ref() == Some(child) {
            return Ok(Some(self.start.checked_add(self.duration)?));
        }
        Ok(None)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExactRepeatLocation {
    pub play: ExactRepeatPlay,
    pub segment_end: ExactRatio,
    /// Exact child-local or gap-local boundary, including either biased edge.
    pub position: ExactRatio,
    pub in_gap: bool,
    pub comparisons: usize,
}

impl ExactRepeatLayout {
    pub fn compile(
        iterations: &IterationOrder,
        child: &NodeId,
        overrides: Option<&PlayOverrides>,
        gap: ExactRatio,
        durations: &BTreeMap<NodeId, ExactRatio>,
    ) -> Result<Self, DocumentError> {
        Self::compile_with_gap_overrides(iterations, child, overrides, gap, None, durations)
    }

    pub fn compile_with_gap_overrides(
        iterations: &IterationOrder,
        child: &NodeId,
        overrides: Option<&PlayOverrides>,
        gap: ExactRatio,
        gap_overrides: Option<&PlayOverrides>,
        durations: &BTreeMap<NodeId, ExactRatio>,
    ) -> Result<Self, DocumentError> {
        Layout::compile(iterations, child, overrides, gap, gap_overrides, durations).map(Self)
    }

    pub fn duration(&self) -> ExactRatio {
        self.0.duration()
    }
    pub fn segment_count(&self) -> usize {
        self.0.segment_count()
    }
    /// Lookup distinguishes a retired identity from arithmetic failure.
    pub fn play(&self, identity: &IterationId) -> Result<Option<ExactRepeatPlay>, DocumentError> {
        self.0.play(identity).map(|play| play.map(Into::into))
    }
    pub fn locate(
        &self,
        position: ExactRatio,
        bias: InsertionBias,
    ) -> Result<ExactRepeatLocation, DocumentError> {
        self.locate_bounded(position, bias, usize::MAX)
    }
    pub fn locate_bounded(
        &self,
        position: ExactRatio,
        bias: InsertionBias,
        maximum_comparisons: usize,
    ) -> Result<ExactRepeatLocation, DocumentError> {
        self.0
            .locate_bounded(position, bias, maximum_comparisons)
            .map(Into::into)
    }
}

impl From<Play<ExactClock>> for ExactRepeatPlay {
    fn from(play: Play<ExactClock>) -> Self {
        Self {
            index: play.index,
            iteration: play.iteration,
            child: play.child,
            start: play.start,
            duration: play.duration,
            gap_after: play.gap_after,
            gap_child: play.gap_child,
        }
    }
}
impl From<Location<ExactClock>> for ExactRepeatLocation {
    fn from(location: Location<ExactClock>) -> Self {
        Self {
            play: location.play.into(),
            segment_end: location.segment_end,
            position: location.position,
            in_gap: location.in_gap,
            comparisons: location.comparisons,
        }
    }
}
