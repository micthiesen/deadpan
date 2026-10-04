//! Saving a [`TrackedPath`] as a core [`AttentionTarget`].
//!
//! The mapping is exact in its rules and bounded in its size:
//!
//! * The first keyframe is the target's `region`; later keyframes are its
//!   `corrections`. Manual samples are not stored as samples.
//! * `tracked` and `interpolated` keep their state; `lost` and `held` both
//!   become core `lost`, which holds the last confident position.
//! * Regions become center and size in millionths of the displayed picture,
//!   each rounded to the nearest millionth with ties away from zero (`f64::round`
//!   of the normalized value times 1,000,000), centers clamped to `0..=1e6`
//!   and sizes to `1..=1e6`.
//! * Confidence becomes thousandths the same way, clamped to `0..=1000`; a
//!   sample without confidence (lost, held) stores 0. Interpolated samples
//!   carry their neighbours' lower confidence, never a rejected observation's.
//! * Samples are compacted to fit the core bounds (4,096 per target, and
//!   whatever the caller's project-wide budget leaves). A sample is dropped
//!   only where core [`AttentionTarget::region_at`] reproduces it: inside a
//!   run of consecutive samples in one moving state with no correction
//!   between, a sample whose center and size components each lie within the
//!   tolerance of the straight line between the kept samples around it
//!   (core rounds that line to the millionth, adding at most one more
//!   millionth); inside a run of `lost` samples holding one region, every
//!   sample after the first. The first and last sample of every run are kept.
//!   Tolerances are tried in order from [`COMPACTION_TOLERANCES`]; if none
//!   fits the budget the mapping fails rather than store a coarser path.
//!   Dropped samples' confidences are not retained.
//! * Provenance records the rule, the tracker of the latest run and why the
//!   first run stopped where the span ends. A re-track replaces the engine and
//!   keeps the stop reason, because the span does not change.

use deadpan_core::{
    AssetId, AttentionTarget, SourceSpan, SourceTimeBase, SourceTimestamp, TARGET_UNITS,
    TargetCorrection, TargetProvenance, TargetRegion, TargetRule, TargetSample, TargetStop,
};

use super::{
    Keyframe, NormalizedRect, TrackError, TrackSample, TrackState, TrackStop, TrackedPath,
};

/// Compaction tolerances in millionths of the picture, tried in order.
pub const COMPACTION_TOLERANCES: [u32; 6] = [500, 1_000, 2_000, 4_000, 8_000, 16_000];
/// The longest stretch one kept segment may replace, bounding compaction work.
const MAX_SEGMENT_SAMPLES: usize = 256;

/// A rectangle as core center and size in millionths.
pub fn target_region(rect: &NormalizedRect) -> TargetRegion {
    let units = f64::from(TARGET_UNITS);
    let scale = |value: f64, minimum: u32| -> u32 {
        (value * units).round().clamp(f64::from(minimum), units) as u32
    };
    let (x, y) = rect.center();
    TargetRegion {
        center: [scale(x, 0), scale(y, 0)],
        size: [scale(rect.width(), 1), scale(rect.height(), 1)],
    }
}

/// The part of a core region inside the picture, as a normalized rectangle.
pub fn rect_from_target(region: &TargetRegion) -> Option<NormalizedRect> {
    let units = f64::from(TARGET_UNITS);
    let [cx, cy] = region.center.map(|value| f64::from(value) / units);
    let [w, h] = region.size.map(|value| f64::from(value) / units);
    NormalizedRect::clipped(cx - w / 2.0, cy - h / 2.0, w, h)
}

/// Confidence in thousandths; absent confidence is 0.
pub fn confidence_thousandths(confidence: Option<f32>) -> u16 {
    confidence
        .filter(|value| value.is_finite())
        .map_or(0, |value| {
            (f64::from(value) * 1_000.0).round().clamp(0.0, 1_000.0) as u16
        })
}

fn core_state(state: TrackState) -> Option<deadpan_core::TrackState> {
    match state {
        TrackState::Manual => None,
        TrackState::Tracked => Some(deadpan_core::TrackState::Tracked),
        TrackState::Interpolated => Some(deadpan_core::TrackState::Interpolated),
        TrackState::Lost | TrackState::Held => Some(deadpan_core::TrackState::Lost),
    }
}

/// A stop reason as recorded in provenance.
pub fn target_stop(stop: TrackStop) -> TargetStop {
    match stop {
        TrackStop::RangeEnd => TargetStop::RangeEnd,
        TrackStop::ShotBoundary { .. } => TargetStop::ShotBoundary,
        TrackStop::PictureLimit => TargetStop::PictureLimit,
    }
}

/// The core rule identity of [`super::TRACK_RULE`].
pub const TARGET_RULE: TargetRule = TargetRule::DeadpanTrack1;

/// Map the non-manual samples, marking where a keyframe (a correction, or
/// the initial region) precedes a sample so runs never span it.
fn mapped(samples: &[TrackSample]) -> Vec<(TargetSample, bool)> {
    let mut out = Vec::with_capacity(samples.len());
    let mut after_keyframe = true;
    for sample in samples {
        let Some(state) = core_state(sample.state) else {
            after_keyframe = true;
            continue;
        };
        out.push((
            TargetSample {
                at: sample.pts,
                region: target_region(&sample.region),
                confidence: confidence_thousandths(sample.confidence),
                state,
            },
            after_keyframe,
        ));
        after_keyframe = false;
    }
    out
}

/// Whether `middle` lies within `tolerance` of the line from `first` to `last`.
fn on_line(
    first: &TargetSample,
    middle: &TargetSample,
    last: &TargetSample,
    tolerance: u32,
) -> bool {
    let t = (middle.at - first.at) as f64 / (last.at - first.at) as f64;
    let close = |a: u32, m: u32, b: u32| {
        let predicted = f64::from(a) + (f64::from(b) - f64::from(a)) * t;
        (predicted - f64::from(m)).abs() <= f64::from(tolerance)
    };
    (0..2).all(|axis| {
        close(
            first.region.center[axis],
            middle.region.center[axis],
            last.region.center[axis],
        ) && close(
            first.region.size[axis],
            middle.region.size[axis],
            last.region.size[axis],
        )
    })
}

/// Drop reconstructible samples at one tolerance.
fn compact_at(samples: &[(TargetSample, bool)], tolerance: u32) -> Vec<TargetSample> {
    let mut kept = Vec::new();
    let mut start = 0;
    while start < samples.len() {
        // A run: same state, no keyframe before any later member.
        let state = samples[start].0.state;
        let mut end = start + 1;
        while end < samples.len() && !samples[end].1 && samples[end].0.state == state {
            end += 1;
        }
        let run = &samples[start..end];
        if state == deadpan_core::TrackState::Lost {
            // Held positions step; keep the first of each held region.
            let mut previous: Option<TargetRegion> = None;
            for (sample, _) in run {
                if previous != Some(sample.region) {
                    kept.push(*sample);
                    previous = Some(sample.region);
                }
            }
        } else {
            let mut anchor = 0;
            kept.push(run[0].0);
            while anchor + 1 < run.len() {
                // Extend the segment from the anchor as far as every sample
                // between stays on the line.
                let mut best = anchor + 1;
                let limit = (anchor + MAX_SEGMENT_SAMPLES).min(run.len() - 1);
                for candidate in anchor + 2..=limit {
                    let fits = (anchor + 1..candidate).all(|middle| {
                        on_line(&run[anchor].0, &run[middle].0, &run[candidate].0, tolerance)
                    });
                    if !fits {
                        break;
                    }
                    best = candidate;
                }
                kept.push(run[best].0);
                anchor = best;
            }
        }
        start = end;
    }
    kept
}

/// Map and compact samples to at most `budget`, returning the samples and
/// the tolerance (millionths) used.
fn compact(
    samples: &[(TargetSample, bool)],
    budget: usize,
) -> Result<(Vec<TargetSample>, u32), TrackError> {
    let budget = budget.min(deadpan_core::MAX_TARGET_SAMPLES);
    for tolerance in COMPACTION_TOLERANCES {
        let kept = compact_at(samples, tolerance);
        if kept.len() <= budget {
            return Ok((kept, tolerance));
        }
    }
    Err(TrackError::Limit("saved target sample"))
}

fn stamp(ticks: i64, time_base: SourceTimeBase) -> SourceTimestamp {
    SourceTimestamp { ticks, time_base }
}

impl TrackedPath {
    /// This path as a core target over `[start_pts, end_pts)` of `asset`, in
    /// the asset's (reduced) time base, with at most `budget` samples. Returns
    /// the target and the compaction tolerance used, in millionths.
    pub fn to_target(
        &self,
        label: String,
        asset: AssetId,
        time_base: SourceTimeBase,
        engine: &str,
        budget: usize,
    ) -> Result<(AttentionTarget, u32), TrackError> {
        if self.keyframes().len() - 1 > deadpan_core::MAX_TARGET_CORRECTIONS {
            return Err(TrackError::Limit("correction"));
        }
        let span = SourceSpan::new(
            stamp(self.start_pts(), time_base),
            stamp(self.end_pts(), time_base),
        )
        .map_err(|_| TrackError::Invalid("the path's range is not a source span"))?;
        let (samples, tolerance) = compact(&mapped(self.samples()), budget)?;
        let target = AttentionTarget {
            label,
            asset,
            span,
            region: target_region(&self.keyframes()[0].region),
            samples,
            corrections: self.keyframes()[1..]
                .iter()
                .map(|keyframe| TargetCorrection {
                    at: keyframe.pts,
                    region: target_region(&keyframe.region),
                })
                .collect(),
            provenance: Some(TargetProvenance {
                rule: TARGET_RULE,
                engine: engine.into(),
                stop: target_stop(self.stop()),
            }),
        };
        Ok((target, tolerance))
    }
}

/// `target` with a correction at `segment`'s start and that correction's
/// range replaced by `segment`, a path tracked from the correction over
/// exactly that range. Samples outside the range and other corrections are
/// unchanged. The replacement holds at most `budget` minus the samples kept
/// outside. Returns the target and the compaction tolerance used.
pub fn retrack_target(
    target: &AttentionTarget,
    segment: &TrackedPath,
    engine: &str,
    budget: usize,
) -> Result<(AttentionTarget, u32), TrackError> {
    let keyframe: Keyframe = segment.keyframes()[0];
    let (start, end) = target
        .correction_range(keyframe.pts)
        .ok_or(TrackError::Invalid(
            "the correction lies outside the target",
        ))?;
    if segment.keyframes().len() != 1
        || segment.start_pts() != start.ticks
        || segment.end_pts() != end.ticks
    {
        return Err(TrackError::Invalid(
            "the re-tracked path must cover exactly the correction's range",
        ));
    }
    let mut outside: Vec<TargetSample> = target
        .samples
        .iter()
        .filter(|sample| sample.at < start.ticks || sample.at >= end.ticks)
        .copied()
        .collect();
    // Before the correction the target may have interpolated from its last
    // kept sample into the replaced range. Nothing interpolates across a
    // correction, so keep that line by ending it one tick before.
    let before = outside.partition_point(|sample| sample.at < start.ticks);
    let last_tick = start.ticks - 1;
    if let Some(previous) = before.checked_sub(1).map(|index| outside[index])
        && previous.at < last_tick
        && let Some(next) = target
            .samples
            .iter()
            .find(|sample| sample.at >= start.ticks)
        && next.state == previous.state
        && let Some((region, deadpan_core::TargetSource::Tracked(state))) =
            target.region_at(deadpan_core::SourcePoint {
                ticks: deadpan_core::ExactRatio::integer(last_tick),
                time_base: start.time_base,
            })
        && state == previous.state
        && region != previous.region
    {
        outside.insert(
            before,
            TargetSample {
                at: last_tick,
                region,
                confidence: previous.confidence.min(next.confidence),
                state,
            },
        );
    }
    let (inside, tolerance) = compact(
        &mapped(segment.samples()),
        budget.saturating_sub(outside.len()),
    )?;
    let mut samples = outside;
    samples.extend(inside);
    samples.sort_by_key(|sample| sample.at);
    let mut corrections: Vec<TargetCorrection> = target
        .corrections
        .iter()
        .filter(|correction| correction.at != keyframe.pts)
        .copied()
        .collect();
    if corrections.len() >= deadpan_core::MAX_TARGET_CORRECTIONS {
        return Err(TrackError::Limit("correction"));
    }
    corrections.push(TargetCorrection {
        at: keyframe.pts,
        region: target_region(&keyframe.region),
    });
    corrections.sort_by_key(|correction| correction.at);
    Ok((
        AttentionTarget {
            samples,
            corrections,
            // The span (and so why it ends) is unchanged; the engine records
            // the latest run.
            provenance: Some(TargetProvenance {
                rule: TARGET_RULE,
                engine: engine.into(),
                stop: target
                    .provenance
                    .as_ref()
                    .map_or(TargetStop::RangeEnd, |provenance| provenance.stop),
            }),
            ..target.clone()
        },
        tolerance,
    ))
}
