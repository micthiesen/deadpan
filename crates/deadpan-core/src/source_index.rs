//! Presentation-order source lookup, independent of a decoder or proxy format.

use serde::{Deserialize, Serialize};

use crate::{AssetId, DocumentError, DocumentErrorCode, ExactRatio, SourceTimeBase};

pub const MAX_SOURCE_INDEX_FRAMES: usize = 10_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SourceFrameId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndexedSourceFrame {
    pub identity: SourceFrameId,
    pub pts: i64,
    /// Observed metadata only. Adjacent presentation timestamps define intervals.
    pub reported_duration: Option<i64>,
    pub keyframe: bool,
    pub seek_from: Option<SourceFrameId>,
    /// Decode order is deliberately independent of presentation order.
    pub decode_timestamp: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalProvenance {
    DecodedFrameDuration,
    PacketDuration,
    StreamEnd,
    ContainerEnd,
    Explicit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourcePoint {
    pub ticks: ExactRatio,
    pub time_base: SourceTimeBase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointPolicy {
    Reject,
    HoldAdjacent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceFrameIndex {
    asset: AssetId,
    time_base: SourceTimeBase,
    frames: Vec<IndexedSourceFrame>,
    terminal_end: i64,
    terminal_provenance: TerminalProvenance,
}

impl SourceFrameIndex {
    /// Source IDs are original presentation ordinals, never proxy frame numbers.
    /// The importer supplies an explicit, measured final boundary and its origin.
    pub fn new(
        asset: AssetId,
        time_base: SourceTimeBase,
        frames: Vec<IndexedSourceFrame>,
        terminal_end: i64,
        terminal_provenance: TerminalProvenance,
    ) -> Result<Self, DocumentError> {
        if frames.is_empty() || frames.len() > MAX_SOURCE_INDEX_FRAMES {
            return Err(invalid("source index must contain 1 to 10,000,000 frames"));
        }
        for (number, frame) in frames.iter().enumerate() {
            if frame.identity.0 != number as u64
                || frame.reported_duration.is_some_and(|value| value <= 0)
            {
                return Err(invalid(
                    "source frame identity or reported duration is invalid",
                ));
            }
            let end = frames.get(number + 1).map_or(terminal_end, |next| next.pts);
            if end <= frame.pts || end.checked_sub(frame.pts).is_none() {
                return Err(invalid(
                    "source presentation intervals must be positive and representable",
                ));
            }
            if let Some(anchor) = frame.seek_from {
                let anchor_index = usize::try_from(anchor.0)
                    .map_err(|_| invalid("seek anchor is outside the source"))?;
                if anchor_index > number
                    || !frames.get(anchor_index).is_some_and(|value| value.keyframe)
                {
                    return Err(invalid(
                        "seek anchor must name an earlier or current keyframe",
                    ));
                }
            }
        }
        Ok(Self {
            asset,
            time_base,
            frames,
            terminal_end,
            terminal_provenance,
        })
    }
    pub fn asset(&self) -> &AssetId {
        &self.asset
    }
    pub fn time_base(&self) -> SourceTimeBase {
        self.time_base
    }
    pub fn frames(&self) -> &[IndexedSourceFrame] {
        &self.frames
    }
    pub fn terminal_end(&self) -> i64 {
        self.terminal_end
    }
    pub fn terminal_provenance(&self) -> TerminalProvenance {
        self.terminal_provenance
    }
    pub fn interval(&self, id: SourceFrameId) -> Result<(i64, i64), DocumentError> {
        let number = usize::try_from(id.0).map_err(|_| invalid("source frame is missing"))?;
        let frame = self
            .frames
            .get(number)
            .ok_or_else(|| invalid("source frame is missing"))?;
        Ok((
            frame.pts,
            self.frames
                .get(number + 1)
                .map_or(self.terminal_end, |next| next.pts),
        ))
    }
    /// O(log source frames), with half-open presentation intervals. A point on
    /// a boundary selects the right-hand frame. Endpoint holding is opt-in.
    pub fn select(
        &self,
        point: SourcePoint,
        endpoints: EndpointPolicy,
    ) -> Result<&IndexedSourceFrame, DocumentError> {
        if point.time_base != self.time_base {
            return Err(invalid("source lookup uses a different timestamp clock"));
        }
        let before = point.ticks.compare_integer(self.frames[0].pts).is_lt();
        let after = !point.ticks.compare_integer(self.terminal_end).is_lt();
        if before || after {
            return match endpoints {
                EndpointPolicy::Reject => Err(invalid(
                    "requested time is outside the source presentation interval",
                )),
                EndpointPolicy::HoldAdjacent => Ok(if before {
                    &self.frames[0]
                } else {
                    &self.frames[self.frames.len() - 1]
                }),
            };
        }
        let right = self
            .frames
            .partition_point(|frame| !point.ticks.compare_integer(frame.pts).is_lt());
        Ok(&self.frames[right - 1])
    }
}

fn invalid(message: &str) -> DocumentError {
    DocumentError::new(DocumentErrorCode::SourceRangeInvalid, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> SourceFrameIndex {
        SourceFrameIndex::new(
            AssetId::new("fixture").unwrap(),
            SourceTimeBase::new(1, 30_000).unwrap(),
            [-2002, -1001, 1001, 4004]
                .into_iter()
                .enumerate()
                .map(|(index, pts)| IndexedSourceFrame {
                    identity: SourceFrameId(index as u64),
                    pts,
                    reported_duration: Some(1001),
                    keyframe: index == 0,
                    seek_from: Some(SourceFrameId(0)),
                    decode_timestamp: Some([0, -2, -1, 1][index]),
                })
                .collect(),
            5005,
            TerminalProvenance::DecodedFrameDuration,
        )
        .unwrap()
    }

    #[test]
    fn vfr_lookup_uses_pts_preserves_negative_origin_and_has_explicit_endpoints() {
        let index = fixture();
        for (ticks, expected) in [
            (-2002, 0),
            (-1002, 0),
            (-1001, 1),
            (0, 1),
            (1001, 2),
            (4003, 2),
            (4004, 3),
        ] {
            let point = SourcePoint {
                ticks: ExactRatio::integer(ticks),
                time_base: index.time_base(),
            };
            assert_eq!(
                index
                    .select(point, EndpointPolicy::Reject)
                    .unwrap()
                    .identity
                    .0,
                expected
            );
        }
        let fractional = SourcePoint {
            ticks: ExactRatio::new(-2003, 2).unwrap(),
            time_base: index.time_base(),
        };
        assert_eq!(
            index
                .select(fractional, EndpointPolicy::Reject)
                .unwrap()
                .identity,
            SourceFrameId(0)
        );
        assert_eq!(index.interval(SourceFrameId(1)).unwrap(), (-1001, 1001));
        for (ticks, expected) in [(-2003, 0), (5005, 3), (i64::MAX, 3)] {
            let point = SourcePoint {
                ticks: ExactRatio::integer(ticks),
                time_base: index.time_base(),
            };
            assert!(index.select(point, EndpointPolicy::Reject).is_err());
            assert_eq!(
                index
                    .select(point, EndpointPolicy::HoldAdjacent)
                    .unwrap()
                    .identity
                    .0,
                expected
            );
        }
    }

    #[test]
    fn malformed_presentation_and_seek_metadata_are_rejected() {
        let base = fixture();
        for mutation in 0..4 {
            let mut frames = base.frames.clone();
            match mutation {
                0 => frames[1].pts = frames[0].pts,
                1 => frames[1].identity = SourceFrameId(0),
                2 => frames[1].seek_from = Some(SourceFrameId(2)),
                _ => frames[2].seek_from = Some(SourceFrameId(1)),
            }
            assert!(
                SourceFrameIndex::new(
                    base.asset.clone(),
                    base.time_base,
                    frames,
                    base.terminal_end,
                    base.terminal_provenance
                )
                .is_err()
            );
        }
        let wrong_clock = SourcePoint {
            ticks: ExactRatio::ZERO,
            time_base: SourceTimeBase::new(1, 1000).unwrap(),
        };
        assert!(base.select(wrong_clock, EndpointPolicy::Reject).is_err());
    }
}
