//! Picture-only cutaways: another moment of the Original shown over part of a
//! host beat while the host's sound continues unchanged.
//!
//! Specification §8.2 "Reaction cutaway" and §17.3: a cutaway is a picture
//! attachment on a Source or Hold, in that beat's local clock, which is its
//! content's clock. It replaces the host's provider picture over a half-open
//! frame range; the host's framing and its ancestors' framing still apply, and
//! no audio changes. The attachment lives on its host node, so moving,
//! copying, splitting (which keeps the full host behind Partitions), trimming,
//! isolating and deleting the host carry it. A shorter cutaway
//! holds its final picture, loops or leaves the host visible, by explicit
//! choice; a longer one is trimmed to its range and to the host.

use serde::{Deserialize, Serialize};

use crate::{
    AssetId, DocumentError, DocumentErrorCode, ExactRatio, ExactSourceSpan, FrameDuration,
    FrameRange, FrameRate, ProjectFrame, SourcePoint, TimeError,
};

/// Cutaways one host may carry.
pub const MAX_CUTAWAYS_PER_NODE: usize = 16;

/// What a cutaway shows after its selected pictures run out.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CutawayFit {
    /// Hold the final selected picture.
    #[default]
    Hold,
    /// Start the selection again.
    Loop,
    /// Show the host's own picture for the rest of the range.
    Gap,
}

impl CutawayFit {
    fn is_hold(&self) -> bool {
        matches!(self, Self::Hold)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cutaway {
    /// Half-open project frames in the host's local output clock.
    pub range: FrameRange,
    /// The Original video asset.
    pub asset: AssetId,
    /// The exact picture interval shown, at its natural rate.
    pub selection: ExactSourceSpan,
    #[serde(default, skip_serializing_if = "CutawayFit::is_hold")]
    pub fit: CutawayFit,
    /// A video-only delete (specification §6.5): the host's picture
    /// contribution is removed over the range, exposing the project
    /// background, while its sound and timing continue. `asset` and
    /// `selection` then record exactly which Original pictures were removed;
    /// nothing is shown from them.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub removed: bool,
}

impl Cutaway {
    /// Source time shown at `local`, a host-local position inside `range`
    /// (picture centers are half frames), or `None` when a `Gap` cutaway has
    /// run out and the host shows through.
    pub fn picture_point(
        &self,
        local: ExactRatio,
        rate: FrameRate,
    ) -> Result<Option<SourcePoint>, TimeError> {
        let start = self.selection.start();
        let end = self.selection.end();
        let base = start.time_base;
        // Source ticks per project frame: (fps_den / fps_num) / (tb_num / tb_den).
        let ticks_per_frame = ExactRatio::new(
            i128::from(rate.denominator()) * i128::from(base.denominator()),
            i128::from(rate.numerator()) * i128::from(base.numerator()),
        )?;
        let offset = local.checked_sub(ExactRatio::integer(self.range.start().0))?;
        let length = end.ticks.checked_sub(start.ticks)?;
        let mut elapsed = offset.checked_mul(ticks_per_frame)?;
        if !elapsed.compare(length).is_lt() {
            match self.fit {
                // The final selected picture: a point at the exclusive end
                // selects it under the adjacent-hold endpoint policy.
                CutawayFit::Hold => elapsed = length,
                CutawayFit::Loop => {
                    let plays = elapsed.checked_div(length)?.floor();
                    elapsed = elapsed.checked_sub(length.checked_mul(ExactRatio::integer(
                        i64::try_from(plays).map_err(|_| TimeError::Overflow)?,
                    ))?)?;
                }
                CutawayFit::Gap => return Ok(None),
            }
        }
        Ok(Some(SourcePoint {
            ticks: start.ticks.checked_add(elapsed)?,
            time_base: base,
        }))
    }

    /// The same cutaway after its host gains `prefix` frames before its
    /// current content.
    pub fn with_owner_prefix(&self, prefix: FrameDuration) -> Result<Self, TimeError> {
        let shift = |frame: ProjectFrame| {
            frame
                .0
                .checked_add(prefix.frames())
                .map(ProjectFrame)
                .ok_or(TimeError::Overflow)
        };
        Ok(Self {
            range: FrameRange::new(shift(self.range.start())?, shift(self.range.end())?)?,
            ..self.clone()
        })
    }
}

/// Shift every cutaway of a host gaining a prefix.
pub fn cutaways_with_owner_prefix(
    cutaways: &[Cutaway],
    prefix: FrameDuration,
) -> Result<Vec<Cutaway>, TimeError> {
    cutaways
        .iter()
        .map(|cutaway| cutaway.with_owner_prefix(prefix))
        .collect()
}

/// Check one host's cutaways: bounded, nonempty, nonnegative, sorted and
/// disjoint, each showing a positive interval of a registered video asset.
pub(crate) fn validate(
    cutaways: &[Cutaway],
    assets: &std::collections::BTreeMap<AssetId, crate::AssetRecord>,
) -> Result<(), DocumentError> {
    let invalid = |message: &str| DocumentError::new(DocumentErrorCode::InvalidTree, message);
    if cutaways.len() > MAX_CUTAWAYS_PER_NODE {
        return Err(DocumentError::new(
            DocumentErrorCode::LimitExceeded,
            "a beat carries at most 16 cutaways",
        ));
    }
    let mut previous: Option<FrameRange> = None;
    for cutaway in cutaways {
        if cutaway.range.start().0 < 0 || cutaway.range.duration() == FrameDuration::ZERO {
            return Err(invalid("a cutaway covers a nonempty nonnegative range"));
        }
        if previous.is_some_and(|previous| previous.end() > cutaway.range.start()) {
            return Err(invalid("cutaways on one beat must be sorted and disjoint"));
        }
        previous = Some(cutaway.range);
        let video = assets
            .get(&cutaway.asset)
            .filter(|asset| !asset.still_image)
            .and_then(|asset| asset.video)
            .ok_or_else(|| {
                DocumentError::new(
                    DocumentErrorCode::MissingAsset,
                    "a cutaway needs a registered video asset",
                )
            })?;
        let start = cutaway.selection.start();
        let end = cutaway.selection.end();
        let within = start.time_base == video.start().time_base
            && !start.ticks.compare_integer(video.start().ticks).is_lt()
            && !end.ticks.compare_integer(video.end().ticks).is_gt();
        if !within {
            return Err(DocumentError::new(
                DocumentErrorCode::SourceRangeInvalid,
                "a cutaway's pictures must lie inside its asset's video",
            ));
        }
    }
    Ok(())
}

/// The Source or Hold a cutaway on `node` belongs to, and the offset of
/// `node`'s clock in the host's: descend through unity Partitions only.
pub fn cutaway_host(
    document: &crate::ProjectDocument,
    node: &crate::NodeId,
) -> Option<(crate::NodeId, i64)> {
    let mut current = node.clone();
    let mut offset = 0_i64;
    for _ in 0..=crate::MAX_DOCUMENT_DEPTH {
        match &document.nodes().get(&current)?.kind {
            crate::NodeKind::Source { .. } | crate::NodeKind::Hold { .. } => {
                return Some((current, offset));
            }
            crate::NodeKind::Retime {
                child,
                duration,
                mapping,
                purpose: crate::RetimePurpose::Partition,
                ..
            } if mapping.duration() == *duration => {
                offset = offset.checked_add(mapping.start().0)?;
                current = child.clone();
            }
            _ => return None,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SourceTimeBase;

    fn point(ticks: i64) -> SourcePoint {
        SourcePoint {
            ticks: ExactRatio::integer(ticks),
            time_base: SourceTimeBase::new(1, 30).unwrap(),
        }
    }

    /// Ten 30 fps pictures from 100 over host frames 4..20 at 30 fps.
    fn cutaway(fit: CutawayFit) -> Cutaway {
        Cutaway {
            range: FrameRange::new(ProjectFrame(4), ProjectFrame(20)).unwrap(),
            asset: AssetId::new("original").unwrap(),
            selection: ExactSourceSpan::new(point(100), point(110)).unwrap(),
            fit,
            removed: false,
        }
    }

    fn shown(cutaway: &Cutaway, frame: i64) -> Option<ExactRatio> {
        cutaway
            .picture_point(
                ExactRatio::new(i128::from(frame) * 2 + 1, 2).unwrap(),
                FrameRate::new(30, 1).unwrap(),
            )
            .unwrap()
            .map(|point| point.ticks)
    }

    #[test]
    fn pictures_play_at_their_natural_rate_then_follow_the_fit() {
        let half = |ticks: i128| Some(ExactRatio::new(ticks * 2 + 1, 2).unwrap());
        let hold = cutaway(CutawayFit::Hold);
        assert_eq!(shown(&hold, 4), half(100));
        assert_eq!(shown(&hold, 13), half(109));
        assert_eq!(shown(&hold, 14), Some(ExactRatio::integer(110)), "held end");
        let looped = cutaway(CutawayFit::Loop);
        assert_eq!(shown(&looped, 14), half(100));
        assert_eq!(shown(&looped, 19), half(105));
        assert_eq!(shown(&cutaway(CutawayFit::Gap), 14), None);
    }

    #[test]
    fn a_prefix_moves_the_range_with_the_host_content() {
        let moved = cutaway(CutawayFit::Hold)
            .with_owner_prefix(FrameDuration::new(3).unwrap())
            .unwrap();
        assert_eq!(
            moved.range,
            FrameRange::new(ProjectFrame(7), ProjectFrame(23)).unwrap()
        );
        assert_eq!(moved.selection, cutaway(CutawayFit::Hold).selection);
    }

    #[test]
    fn the_wire_omits_the_default_fit() {
        let json = serde_json::to_string(&cutaway(CutawayFit::Hold)).unwrap();
        assert!(!json.contains("fit"), "{json}");
        let looped = serde_json::to_string(&cutaway(CutawayFit::Loop)).unwrap();
        assert_eq!(
            serde_json::from_str::<Cutaway>(&looped).unwrap(),
            cutaway(CutawayFit::Loop)
        );
    }
}
