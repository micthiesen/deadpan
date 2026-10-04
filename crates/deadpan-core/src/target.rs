//! Attention targets: a region of the Original followed over source time.
//!
//! Specification §4 and §11.3: an AttentionTarget names a source asset, a source-time
//! range, its tracked region per picture (sparse or per frame) with
//! confidence and tracking state, and manual corrections. It is evaluated in
//! source time, so a target follows its subject through any retiming or
//! repetition of the pictures that show it. Targets carry no identity of the
//! person they follow.
//!
//! Regions use the same normalized, uncropped source coordinates as framing
//! centers, at a fixed precision of one millionth: `(0, 0)` is the top-left
//! corner and `(1, 1)` the bottom-right corner of the displayed picture.

use serde::{Deserialize, Serialize};

use crate::{
    AssetId, DocumentError, DocumentErrorCode, ExactRatio, SourcePoint, SourceSpan,
    SourceTimestamp, TargetId,
};

/// Units per normalized coordinate.
pub const TARGET_UNITS: u32 = 1_000_000;
/// Targets one project may hold.
pub const MAX_DOCUMENT_TARGETS: usize = 64;
/// Tracked samples one target may hold; hosts compact denser paths.
pub const MAX_TARGET_SAMPLES: usize = 4_096;
/// Tracked samples a project may hold across its targets.
pub const MAX_DOCUMENT_TARGET_SAMPLES: usize = 32_768;
/// Manual corrections one target may hold.
pub const MAX_TARGET_CORRECTIONS: usize = 256;

/// A rectangle by center and size, in millionths of the displayed picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetRegion {
    pub center: [u32; 2],
    pub size: [u32; 2],
}

impl TargetRegion {
    pub fn validate(&self) -> Result<(), DocumentError> {
        let inside = self.center.iter().all(|value| *value <= TARGET_UNITS)
            && self
                .size
                .iter()
                .all(|value| (1..=TARGET_UNITS).contains(value));
        if inside {
            Ok(())
        } else {
            Err(invalid(
                "a target region has its center inside the picture and a positive size",
            ))
        }
    }

    /// The center as exact normalized framing coordinates.
    pub fn center_ratio(&self) -> [ExactRatio; 2] {
        self.center.map(|value| {
            ExactRatio::new(i128::from(value), i128::from(TARGET_UNITS))
                .expect("constant positive denominator")
        })
    }
}

/// How a tracked sample was obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackState {
    /// The tracker followed the subject with confidence.
    Tracked,
    /// A short low-confidence gap filled between confident neighbours.
    Interpolated,
    /// The subject was lost; the region holds the last confident position
    /// until a manual correction.
    Lost,
}

/// One tracked picture: its source time, region, confidence and state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetSample {
    /// Source ticks in the target span's time base.
    pub at: i64,
    pub region: TargetRegion,
    /// Tracker confidence in thousandths.
    pub confidence: u16,
    pub state: TrackState,
}

/// A manual region from a source time onward, overriding tracked samples
/// until the next correction or tracked sample after it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetCorrection {
    pub at: i64,
    pub region: TargetRegion,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttentionTarget {
    pub label: String,
    pub asset: AssetId,
    /// The half-open source range the target covers.
    pub span: SourceSpan,
    /// The region selected when the target was created, at the span start.
    pub region: TargetRegion,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub samples: Vec<TargetSample>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub corrections: Vec<TargetCorrection>,
}

/// The region a target gives one source time, and where it came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetSource {
    Initial,
    Manual,
    Tracked(TrackState),
}

impl AttentionTarget {
    pub(crate) fn validate(
        &self,
        assets: &std::collections::BTreeMap<AssetId, crate::AssetRecord>,
    ) -> Result<(), DocumentError> {
        if self.label.trim().is_empty() || self.label.len() > 128 {
            return Err(invalid("a target label is 1–128 bytes"));
        }
        let video = assets
            .get(&self.asset)
            .filter(|asset| !asset.still_image)
            .and_then(|asset| asset.video)
            .ok_or_else(|| {
                DocumentError::new(
                    DocumentErrorCode::MissingAsset,
                    "a target follows a registered video asset",
                )
            })?;
        let (start, end) = (self.span.start(), self.span.end());
        if start.time_base != video.start().time_base
            || start.ticks < video.start().ticks
            || end.ticks > video.end().ticks
        {
            return Err(DocumentError::new(
                DocumentErrorCode::SourceRangeInvalid,
                "a target's span lies inside its asset's video",
            ));
        }
        self.region.validate()?;
        if self.samples.len() > MAX_TARGET_SAMPLES
            || self.corrections.len() > MAX_TARGET_CORRECTIONS
        {
            return Err(DocumentError::new(
                DocumentErrorCode::LimitExceeded,
                "a target holds at most 4096 samples and 256 corrections",
            ));
        }
        let within = |at: i64| at >= start.ticks && at < end.ticks;
        let mut previous = None;
        for sample in &self.samples {
            sample.region.validate()?;
            if !within(sample.at)
                || previous.is_some_and(|previous| previous >= sample.at)
                || sample.confidence > 1000
            {
                return Err(invalid(
                    "target samples are strictly ordered inside the span with confidence ≤ 1000",
                ));
            }
            previous = Some(sample.at);
        }
        let mut previous = None;
        for correction in &self.corrections {
            correction.region.validate()?;
            if !within(correction.at) || previous.is_some_and(|previous| previous >= correction.at)
            {
                return Err(invalid(
                    "target corrections are strictly ordered inside the span",
                ));
            }
            previous = Some(correction.at);
        }
        Ok(())
    }

    /// The region at source time `point`, or `None` outside the span or in
    /// another time base. The latest correction or sample at or before the
    /// point applies; a correction wins over a sample at the same time.
    pub fn region_at(&self, point: SourcePoint) -> Option<(TargetRegion, TargetSource)> {
        let (start, end) = (self.span.start(), self.span.end());
        if point.time_base != start.time_base
            || point.ticks.compare_integer(start.ticks).is_lt()
            || !point.ticks.compare_integer(end.ticks).is_lt()
        {
            return None;
        }
        // Both lists are strictly ordered: take the last entry at or before.
        let at_or_before = |at: i64| !point.ticks.compare_integer(at).is_lt();
        let sample = self
            .samples
            .partition_point(|sample| at_or_before(sample.at))
            .checked_sub(1)
            .map(|index| &self.samples[index]);
        let correction = self
            .corrections
            .partition_point(|correction| at_or_before(correction.at))
            .checked_sub(1)
            .map(|index| &self.corrections[index]);
        Some(match (sample, correction) {
            (Some(sample), Some(correction)) if sample.at > correction.at => {
                (sample.region, TargetSource::Tracked(sample.state))
            }
            (_, Some(correction)) => (correction.region, TargetSource::Manual),
            (Some(sample), None) => (sample.region, TargetSource::Tracked(sample.state)),
            (None, None) => (self.region, TargetSource::Initial),
        })
    }

    /// The tracked range a correction at `at` invalidates: from it to the next
    /// correction, or the end of the span. The host replaces samples in that
    /// range by re-tracking from the correction; samples after a correction
    /// are therefore its re-tracked continuation and apply after it.
    pub fn correction_range(&self, at: i64) -> Option<(SourceTimestamp, SourceTimestamp)> {
        let (start, end) = (self.span.start(), self.span.end());
        if at < start.ticks || at >= end.ticks {
            return None;
        }
        let next = self
            .corrections
            .iter()
            .map(|correction| correction.at)
            .find(|next| *next > at)
            .unwrap_or(end.ticks);
        Some((
            SourceTimestamp {
                ticks: at,
                time_base: start.time_base,
            },
            SourceTimestamp {
                ticks: next,
                time_base: start.time_base,
            },
        ))
    }
}

/// Check every target and the document-wide bounds.
pub(crate) fn validate_targets(
    targets: &std::collections::BTreeMap<TargetId, AttentionTarget>,
    assets: &std::collections::BTreeMap<AssetId, crate::AssetRecord>,
) -> Result<(), DocumentError> {
    if targets.len() > MAX_DOCUMENT_TARGETS
        || targets
            .values()
            .map(|target| target.samples.len())
            .sum::<usize>()
            > MAX_DOCUMENT_TARGET_SAMPLES
    {
        return Err(DocumentError::new(
            DocumentErrorCode::LimitExceeded,
            "a project holds at most 64 targets and 32768 tracked samples",
        ));
    }
    targets
        .values()
        .try_for_each(|target| target.validate(assets))
}

fn invalid(message: &str) -> DocumentError {
    DocumentError::new(DocumentErrorCode::InvalidTree, message)
}

#[cfg(test)]
mod tests;
