//! Cursor-relative range intent and canonical, finished Edit geometry.

use crate::{
    DocumentError, EditError, EditErrorCode, FrameDuration, FrameRange, ProjectDocument,
    ProjectFrame, SemanticContext, SemanticVisualSelection,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct SemanticFrameOffset(pub i64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticRangeOffsets {
    pub start: SemanticFrameOffset,
    pub end: SemanticFrameOffset,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticRangeSetTarget {
    pub ranges: Vec<FrameRange>,
    pub total_duration: FrameDuration,
}

pub(super) fn validate_offsets(ranges: &[SemanticRangeOffsets]) -> Result<(), EditError> {
    if ranges.is_empty() || ranges.len() > crate::MAX_EDIT_SET_COMPONENTS {
        return Err(EditError::new(
            EditErrorCode::LimitExceeded,
            "a range set requires 1..=64 ranges",
        ));
    }
    if ranges.iter().any(|range| range.start.0 >= range.end.0) {
        return Err(unavailable(
            "every range set interval must have positive duration",
        ));
    }
    Ok(())
}

fn unavailable(message: &str) -> EditError {
    EditError::new(EditErrorCode::SelectionUnavailable, message)
}

pub(super) fn deserialize_offsets<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<SemanticRangeOffsets>, D::Error> {
    struct Ranges;
    impl<'de> serde::de::Visitor<'de> for Ranges {
        type Value = Vec<SemanticRangeOffsets>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("1..=64 positive cursor-relative range pairs")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut input: A,
        ) -> Result<Self::Value, A::Error> {
            let mut ranges = Vec::new();
            while let Some(range) = input.next_element()? {
                if ranges.len() == crate::MAX_EDIT_SET_COMPONENTS {
                    return Err(serde::de::Error::custom("range set interval limit"));
                }
                ranges.push(range);
            }
            validate_offsets(&ranges).map_err(serde::de::Error::custom)?;
            Ok(ranges)
        }
    }
    deserializer.deserialize_seq(Ranges)
}

pub(super) fn deserialize_geometry<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<FrameRange>, D::Error> {
    struct Geometry;
    impl<'de> serde::de::Visitor<'de> for Geometry {
        type Value = Vec<FrameRange>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("1..=64 canonical positive Edit intervals")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut input: A,
        ) -> Result<Self::Value, A::Error> {
            let mut ranges: Vec<FrameRange> = Vec::new();
            while let Some(range) = input.next_element::<FrameRange>()? {
                if ranges.len() == crate::MAX_EDIT_SET_COMPONENTS
                    || range.start() >= range.end()
                    || ranges
                        .last()
                        .is_some_and(|last| last.end() >= range.start())
                {
                    return Err(serde::de::Error::custom(
                        "finished range set geometry is not bounded and canonical",
                    ));
                }
                ranges.push(range);
            }
            if ranges.is_empty() {
                return Err(serde::de::Error::custom("empty finished range set"));
            }
            Ok(ranges)
        }
    }
    deserializer.deserialize_seq(Geometry)
}

pub(super) fn validate_geometry(
    document: &ProjectDocument,
    context: &SemanticContext,
    ranges: &[FrameRange],
) -> Result<SemanticRangeSetTarget, EditError> {
    super::node::edit_authority(context)?;
    let bounds = super::planner::scope_bounds(document, &context.parent)?;
    if ranges.is_empty() || ranges.len() > crate::MAX_EDIT_SET_COMPONENTS {
        return Err(unavailable(
            "a finished range set requires 1..=64 intervals",
        ));
    }
    let mut previous_end = None;
    let mut total = 0i64;
    for range in ranges {
        if range.start() >= range.end()
            || range.start() < bounds.0
            || range.end() > bounds.1
            || previous_end.is_some_and(|end| end >= range.start())
        {
            return Err(unavailable(
                "finished range set geometry must be canonical and inside its Sequence",
            ));
        }
        // Use the same endpoint and temporary Split budget contract as an
        // ordinary linked range cut, even for a capture-only instruction.
        document.range_deletion(&context.parent, *range)?;
        total = total
            .checked_add(range.duration().frames())
            .ok_or_else(|| {
                EditError::new(EditErrorCode::TimingOverflow, "range set duration overflow")
            })?;
        previous_end = Some(range.end());
    }
    Ok(SemanticRangeSetTarget {
        ranges: ranges.to_vec(),
        total_duration: FrameDuration::new(total).map_err(DocumentError::from)?,
    })
}

pub(super) fn resolve_in_scope(
    document: &ProjectDocument,
    context: &SemanticContext,
    offsets: &[SemanticRangeOffsets],
) -> Result<SemanticRangeSetTarget, EditError> {
    super::node::edit_authority(context)?;
    validate_offsets(offsets)?;
    let bounds = super::planner::scope_bounds(document, &context.parent)?;
    let mut ranges = Vec::with_capacity(offsets.len());
    // Check every original pair before normalization: a merged interval must
    // never conceal an overflowing or out-of-scope input.
    for pair in offsets {
        let add = |offset: SemanticFrameOffset| {
            context
                .cursor
                .0
                .checked_add(offset.0)
                .map(ProjectFrame)
                .ok_or_else(|| {
                    EditError::new(
                        EditErrorCode::TimingOverflow,
                        "cursor-relative range boundary overflow",
                    )
                })
        };
        let range =
            FrameRange::new(add(pair.start)?, add(pair.end)?).map_err(DocumentError::from)?;
        if range.start() < bounds.0 || range.end() > bounds.1 {
            return Err(unavailable(
                "a range set interval is outside its current Sequence",
            ));
        }
        ranges.push(range);
    }
    ranges.sort_by_key(|range| (range.start(), range.end()));
    let mut canonical: Vec<FrameRange> = Vec::new();
    for range in ranges {
        if let Some(last) = canonical.last_mut()
            && range.start() <= last.end()
        {
            *last = FrameRange::new(last.start(), last.end().max(range.end()))
                .map_err(DocumentError::from)?;
        } else {
            canonical.push(range);
        }
    }
    validate_geometry(document, context, &canonical)
}

impl ProjectDocument {
    pub fn resolve_semantic_range_selection(
        &self,
        context: &SemanticContext,
        ranges: &[FrameRange],
    ) -> Result<SemanticRangeSetTarget, EditError> {
        super::planner::validate_context(self, context)?;
        validate_geometry(self, context, ranges)
    }
    pub fn resolve_semantic_ranges(
        &self,
        context: &SemanticContext,
        ranges: &[SemanticRangeOffsets],
    ) -> Result<SemanticRangeSetTarget, EditError> {
        super::planner::validate_context(self, context)?;
        resolve_in_scope(self, context, ranges)
    }

    pub fn select_semantic_ranges(
        &self,
        context: &SemanticContext,
        ranges: Vec<SemanticRangeOffsets>,
    ) -> Result<SemanticContext, EditError> {
        let mut result = context.clone();
        result.visual_selection = None;
        let target = self.resolve_semantic_ranges(&result, &ranges)?;
        result.cursor = target.ranges.last().expect("nonempty checked set").end();
        let bounds = super::planner::scope_bounds(self, &result.parent)?;
        let ends = super::planner::child_ends(self, &result.parent, bounds)?;
        result.selected_child = super::planner::selected_child(&ends, result.cursor, bounds);
        result.visual_selection = Some(SemanticVisualSelection::Ranges {
            ranges: target.ranges,
        });
        Ok(result)
    }
}
