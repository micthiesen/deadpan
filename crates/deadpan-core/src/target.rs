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
/// Bytes of each provenance label.
pub const MAX_TARGET_PROVENANCE_BYTES: usize = 96;

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

/// The versioned tracking policy that produced a target's samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TargetRule {
    #[serde(rename = "deadpan-track-1")]
    DeadpanTrack1,
}

/// Why tracking ended where the target's span ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetStop {
    /// The requested range ended.
    RangeEnd,
    /// A shot boundary began the next shot.
    ShotBoundary,
    /// The tracker's picture limit was reached.
    PictureLimit,
}

/// How a target's samples were produced, for display and re-tracking. It is
/// a record, never an authorization; a hand-made target has none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetProvenance {
    pub rule: TargetRule,
    /// The tracker of the latest run over any of the samples, such as
    /// `Apple Vision VNTrackObjectRequest 2 accurate`.
    pub engine: String,
    /// Why the first run, which set the span's end, stopped there.
    pub stop: TargetStop,
}

impl TargetProvenance {
    fn validate(&self) -> Result<(), DocumentError> {
        // One printable line: no control characters and no Unicode line or
        // paragraph separators.
        let printable = !self.engine.is_empty()
            && self.engine.len() <= MAX_TARGET_PROVENANCE_BYTES
            && !self
                .engine
                .chars()
                .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}'));
        if printable {
            Ok(())
        } else {
            Err(invalid(
                "a target provenance engine is one printable line of 1–96 bytes",
            ))
        }
    }
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<TargetProvenance>,
}

/// The region a target gives one source time, and where it came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetSource {
    Initial,
    Manual,
    Tracked(TrackState),
}

/// Exact target evaluation with the weakest confidence supporting its region.
/// Authored regions have no detector confidence; lost samples remain explicit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvaluatedTargetRegion {
    pub region: TargetRegion,
    pub source: TargetSource,
    pub confidence: Option<u16>,
}

impl AttentionTarget {
    /// Validate the retained record without a document's asset catalog. This
    /// checks labels, regions, sample ordering/confidence and provenance only;
    /// document admission additionally proves the registered video and range.
    pub fn validate_shape(&self) -> Result<(), DocumentError> {
        self.validate_label()?;
        self.validate_observations()
    }

    fn validate_label(&self) -> Result<(), DocumentError> {
        if self.label.trim().is_empty() || self.label.len() > 128 {
            return Err(invalid("a target label is 1–128 bytes"));
        }
        Ok(())
    }

    pub(crate) fn validate(
        &self,
        assets: &std::collections::BTreeMap<AssetId, crate::AssetRecord>,
    ) -> Result<(), DocumentError> {
        self.validate_label()?;
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
        self.validate_observations()
    }

    fn validate_observations(&self) -> Result<(), DocumentError> {
        let (start, end) = (self.span.start(), self.span.end());
        self.region.validate()?;
        if let Some(provenance) = &self.provenance {
            provenance.validate()?;
        }
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
    ///
    /// Between two consecutive samples in the same moving state (both
    /// `Tracked` or both `Interpolated`) with no correction after the first
    /// one up to the second, the region is interpolated linearly in source
    /// time, each center and size component rounded half to even to a
    /// millionth, so sparse or strided paths do not step. Nothing is
    /// interpolated across a `Lost` sample, a change of state, a correction or
    /// from the initial region; there the earlier entry holds.
    ///
    /// Where that exact interpolation cannot be represented (an extreme
    /// fractional source point overflows the exact arithmetic), the region is
    /// unavailable and the result is `None`, as outside the span: a caller
    /// such as `Follow` framing then uses its fallback rather than a region the
    /// target does not describe.
    pub fn region_at(&self, point: SourcePoint) -> Option<(TargetRegion, TargetSource)> {
        self.evaluated_region_at(point)
            .map(|value| (value.region, value.source))
    }

    pub fn evaluated_region_at(&self, point: SourcePoint) -> Option<EvaluatedTargetRegion> {
        let (start, end) = (self.span.start(), self.span.end());
        if point.time_base != start.time_base
            || point.ticks.compare_integer(start.ticks).is_lt()
            || !point.ticks.compare_integer(end.ticks).is_lt()
        {
            return None;
        }
        // Both lists are strictly ordered: take the last entry at or before.
        let at_or_before = |at: i64| !point.ticks.compare_integer(at).is_lt();
        let sample_index = self
            .samples
            .partition_point(|sample| at_or_before(sample.at))
            .checked_sub(1);
        let sample = sample_index.map(|index| &self.samples[index]);
        let correction = self
            .corrections
            .partition_point(|correction| at_or_before(correction.at))
            .checked_sub(1)
            .map(|index| &self.corrections[index]);
        let tracked = |sample: &TargetSample| -> Option<EvaluatedTargetRegion> {
            let (region, confidence) = self.sample_region(sample_index?, point)?;
            Some(EvaluatedTargetRegion {
                region,
                source: TargetSource::Tracked(sample.state),
                confidence: Some(confidence),
            })
        };
        match (sample, correction) {
            (Some(sample), Some(correction)) if sample.at > correction.at => tracked(sample),
            (_, Some(correction)) => Some(EvaluatedTargetRegion {
                region: correction.region,
                source: TargetSource::Manual,
                confidence: None,
            }),
            (Some(sample), None) => tracked(sample),
            (None, None) => Some(EvaluatedTargetRegion {
                region: self.region,
                source: TargetSource::Initial,
                confidence: None,
            }),
        }
    }

    /// The region from sample `index` (the last at or before `point`),
    /// interpolated towards the next sample where both move together, or
    /// `None` when that interpolation overflows.
    fn sample_region(&self, index: usize, point: SourcePoint) -> Option<(TargetRegion, u16)> {
        let first = &self.samples[index];
        let Some(next) = self.samples.get(index + 1) else {
            return Some((first.region, first.confidence));
        };
        let moving = matches!(first.state, TrackState::Tracked | TrackState::Interpolated);
        let corrected = self
            .corrections
            .iter()
            .any(|correction| correction.at > first.at && correction.at <= next.at);
        if !moving
            || next.state != first.state
            || corrected
            || point.ticks.compare_integer(first.at).is_eq()
        {
            return Some((first.region, first.confidence));
        }
        let fraction = point
            .ticks
            .checked_sub(ExactRatio::integer(first.at))
            .and_then(|offset| {
                offset.checked_div(ExactRatio::new(
                    i128::from(next.at) - i128::from(first.at),
                    1,
                )?)
            });
        let fraction = fraction.ok()?;
        let mix = |from: u32, to: u32, minimum: u32| -> Option<u32> {
            let delta = ExactRatio::integer(i64::from(to) - i64::from(from));
            let value = ExactRatio::integer(i64::from(from))
                .checked_add(delta.checked_mul(fraction).ok()?)
                .ok()?
                .round_even()
                .ok()?;
            u32::try_from(value.clamp(i128::from(minimum), i128::from(TARGET_UNITS))).ok()
        };
        Some((
            TargetRegion {
                center: [
                    mix(first.region.center[0], next.region.center[0], 0)?,
                    mix(first.region.center[1], next.region.center[1], 0)?,
                ],
                size: [
                    mix(first.region.size[0], next.region.size[0], 1)?,
                    mix(first.region.size[1], next.region.size[1], 1)?,
                ],
            },
            first.confidence.min(next.confidence),
        ))
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
