//! Incremental tracking by bounded, cursor-priority ranges (specification
//! §11.4; [`deadpan_analysis::RangePlan`]).
//!
//! Every range is one supervised worker run over the prepared attempt copy,
//! seeded by its keyframe or by the previous range's last tracker report.
//! Each completed range's raw observations are kept in the private,
//! rebuildable `AnalysisRanges` cache, keyed by the Original's content,
//! stream and interpretation, the stride, the segment's keyframe (the target's
//! identity as tracked), the owned pictures and the exact seed. No authored
//! state, target ID or revision enters the key, so a cancelled or interrupted
//! run resumes from its completed ranges and an edit elsewhere keeps them. A
//! cache failure costs only resumability. The policy is applied to the
//! completed prefix after every range, so callers can show partial coverage;
//! only a complete plan is returned as the result.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::time::Instant;

use deadpan_analysis::{
    Keyframe, PlannedRange, RangePlan, RawObservation, TRACK_RULE, TrackPolicy, TrackedPath,
    continuation_region,
};
use deadpan_jobs::tracking::{self as protocol, ExpectedStream};
use serde::{Deserialize, Serialize};

use super::{PreparedTrack, TrackingError, TrackingRuntime, check, engine_label, run_worker};
use crate::transcription::incremental::cache::RangeCache;

/// Bumped when range results change meaning.
const IMPLEMENTATION: &str = "deadpan-track-ranges-1";

/// Where completed ranges are kept and how wide they are.
#[derive(Debug, Clone)]
pub struct RangeOptions {
    /// The private range cache; `None` keeps results in memory only.
    pub cache_directory: Option<PathBuf>,
    /// Pictures per range, rounded up to whole strides.
    pub range_pictures: usize,
}

impl Default for RangeOptions {
    fn default() -> Self {
        Self {
            cache_directory: None,
            range_pictures: deadpan_analysis::TRACK_RANGE_PICTURES,
        }
    }
}

/// Coverage after a range completes.
pub struct RangeProgress<'a> {
    /// Completed and total ranges.
    pub completed: usize,
    pub total: usize,
    /// Ranges restored from the cache so far.
    pub cached: usize,
    /// Every completed range as a half-open PTS span, in picture order.
    pub coverage: Vec<(i64, i64)>,
    /// The policy applied to the completed prefix from the first keyframe,
    /// when there is one. Unfinished ranges never contribute.
    pub partial: Option<&'a TrackedPath>,
    /// The tracker of the latest range, as provenance records it.
    pub engine: Option<&'a str>,
}

/// A complete incremental run.
#[derive(Debug, Clone)]
pub struct RangesResult {
    pub path: TrackedPath,
    /// The tracker of the latest range.
    pub engine: String,
    pub ranges: usize,
    pub cached_ranges: usize,
    pub cache_failures: usize,
}

/// One range's observations and the engine label that produced them.
pub type RangeRun = Result<(Vec<RawObservation>, String), TrackingError>;

/// One tracker run: `pictures` (every indexed PTS from the seed to before
/// `end_pts`), seeded on its first picture. Returns one observation per
/// analysed picture (every stride-th from the seed) and the engine label.
pub type RunRange<'a> = dyn FnMut(&[i64], i64, Keyframe, &mut dyn FnMut(u8)) -> RangeRun + 'a;

/// The installed worker as a [`RunRange`], one attempt name per range.
pub fn worker_runs<'a>(
    runtime: &'a TrackingRuntime,
    prepared: &'a PreparedTrack,
    attempt: &'a str,
    cancelled: &'a AtomicBool,
    deadline: Instant,
) -> impl FnMut(&[i64], i64, Keyframe, &mut dyn FnMut(u8)) -> RangeRun + 'a {
    let mut run = 0_usize;
    move |pictures, end_pts, seed, progress| {
        run += 1;
        let result = run_worker(
            runtime,
            prepared,
            pictures,
            end_pts,
            seed,
            &format!("{attempt}-r{run}"),
            cancelled,
            deadline,
            progress,
        )?;
        Ok((result.observations, engine_label(&result.runtime)))
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
struct Key {
    implementation: String,
    rule: String,
    protocol: u32,
    content: String,
    stream: ExpectedStream,
    stride: u32,
    /// The keyframe the range's segment is tracked from.
    keyframe: Keyframe,
    /// Every owned picture, in order.
    owned: Vec<i64>,
    /// The picture and region the run was seeded with.
    seed: Keyframe,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    key: Key,
    engine: String,
    observations: Vec<RawObservation>,
}

/// Track `prepared` from `keyframes` (the first at the prepared start, each at
/// an indexed picture of the range) by ranges, nearest `cursor` (a source PTS,
/// read before every range) first. `boundary` runs before every range and may
/// wait or stop; `progress` reports each completed range.
#[allow(clippy::too_many_arguments)]
pub fn track_ranges(
    prepared: &PreparedTrack,
    keyframes: &[Keyframe],
    policy: TrackPolicy,
    options: &RangeOptions,
    cursor: &AtomicI64,
    tracker: &mut RunRange<'_>,
    cancelled: &AtomicBool,
    deadline: Instant,
    mut boundary: impl FnMut() -> Result<(), TrackingError>,
    mut percent: impl FnMut(u8),
    mut progress: impl FnMut(RangeProgress<'_>),
) -> Result<RangesResult, TrackingError> {
    policy.validate()?;
    check(cancelled, deadline)?;
    let pictures = prepared.pictures();
    if keyframes.first().map(|keyframe| keyframe.pts) != Some(prepared.start_pts())
        || keyframes.first().map(|keyframe| keyframe.region) != Some(prepared.region)
    {
        return Err(TrackingError::Request(
            "the first keyframe must be the prepared selection".into(),
        ));
    }
    let ordinals = keyframes
        .iter()
        .map(|keyframe| pictures.binary_search(&keyframe.pts))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| TrackingError::Request("a keyframe must be at an indexed picture".into()))?;
    let plan = RangePlan::new(
        pictures.len(),
        &ordinals,
        prepared.stride,
        options.range_pictures,
    )?;
    let total_analysed: usize = (0..plan.ranges().len())
        .map(|index| plan.analysed(index).count())
        .sum();
    let mut cache_failures = 0;
    let cache = options
        .cache_directory
        .as_ref()
        .and_then(|path| match RangeCache::open(path) {
            Ok(cache) => Some(cache),
            Err(_) => {
                cache_failures += 1;
                None
            }
        });
    let count = plan.ranges().len();
    let mut observations: Vec<Option<Vec<RawObservation>>> = vec![None; count];
    // The seed each completed range was run from, for its continuation.
    let mut seeds: Vec<Option<Keyframe>> = vec![None; count];
    let mut completed = vec![false; count];
    let mut cached_ranges = 0;
    let mut engine: Option<String> = None;
    let mut done_analysed = 0_usize;
    loop {
        let at = cursor.load(Ordering::Acquire);
        let cursor_ordinal = pictures.partition_point(|&pts| pts <= at).saturating_sub(1);
        let Some(index) = plan.next(&completed, cursor_ordinal) else {
            break;
        };
        boundary()?;
        check(cancelled, deadline)?;
        let range = plan.ranges()[index];
        let keyframe = keyframes[range.segment];
        let seed = if range.continues() {
            let previous = index - 1;
            Keyframe {
                pts: pictures[range.seed],
                region: continuation_region(
                    seeds[previous]
                        .expect("a ready range's predecessor completed")
                        .region,
                    observations[previous]
                        .as_deref()
                        .expect("a ready range's predecessor completed"),
                ),
            }
        } else {
            keyframe
        };
        let owned_analysed: Vec<i64> = plan
            .analysed(index)
            .map(|ordinal| pictures[ordinal])
            .collect();
        let key = Key {
            implementation: IMPLEMENTATION.into(),
            rule: TRACK_RULE.into(),
            protocol: protocol::VERSION,
            content: prepared.content.clone(),
            stream: prepared.stream,
            stride: prepared.stride,
            keyframe,
            owned: pictures[range.first..range.end].to_vec(),
            seed,
        };
        let name = RangeCache::name(&key).ok();
        let cached = cache
            .as_ref()
            .zip(name.as_deref())
            .and_then(|(cache, name)| match cache.read::<Record>(name) {
                Ok(Some(record))
                    if record.key == key
                        && record
                            .observations
                            .iter()
                            .map(|observation| observation.pts)
                            .eq(owned_analysed.iter().copied()) =>
                {
                    Some(record)
                }
                Ok(None) => None,
                Ok(Some(_)) | Err(_) => {
                    cache_failures += 1;
                    None
                }
            });
        let (owned, label) = if let Some(record) = cached {
            cached_ranges += 1;
            (record.observations, record.engine)
        } else {
            let run_end = range_end(&plan, range, pictures, prepared.end_pts());
            let run_pictures = &pictures[range.seed..range.end];
            let base = done_analysed;
            let range_analysed = owned_analysed.len();
            let mut report = |worker: u8| {
                let within = range_analysed * usize::from(worker.min(100)) / 100;
                percent(((base + within) * 100 / total_analysed.max(1)).min(99) as u8);
            };
            let (mut reported, label) = tracker(run_pictures, run_end, seed, &mut report)?;
            check(cancelled, deadline)?;
            if range.continues() {
                // The seed picture's report belongs to the previous range.
                if reported.first().map(|observation| observation.pts) != Some(seed.pts) {
                    return Err(TrackingError::Protocol(
                        "a continuation did not report its seed picture".into(),
                    ));
                }
                reported.remove(0);
            }
            if !reported
                .iter()
                .map(|observation| observation.pts)
                .eq(owned_analysed.iter().copied())
            {
                return Err(TrackingError::Protocol(
                    "range observations do not match the indexed pictures requested".into(),
                ));
            }
            if let (Some(cache), Some(name)) = (&cache, &name)
                && cache
                    .write(
                        name,
                        &Record {
                            key: key.clone(),
                            engine: label.clone(),
                            observations: reported.clone(),
                        },
                    )
                    .is_err()
            {
                cache_failures += 1;
            }
            (reported, label)
        };
        done_analysed += owned_analysed.len();
        observations[index] = Some(owned);
        seeds[index] = Some(seed);
        completed[index] = true;
        engine = Some(label);
        let partial = plan.assemble(
            policy,
            prepared.display_aspect,
            pictures,
            prepared.end_pts(),
            prepared.stop(),
            keyframes,
            &observations,
        )?;
        percent((done_analysed * 100 / total_analysed.max(1)).min(100) as u8);
        progress(RangeProgress {
            completed: completed.iter().filter(|done| **done).count(),
            total: count,
            cached: cached_ranges,
            coverage: coverage(&plan, &completed, pictures, prepared.end_pts()),
            partial: partial.as_ref(),
            engine: engine.as_deref(),
        });
    }
    check(cancelled, deadline)?;
    let path = plan
        .assemble(
            policy,
            prepared.display_aspect,
            pictures,
            prepared.end_pts(),
            prepared.stop(),
            keyframes,
            &observations,
        )?
        .filter(|path| path.end_pts() == prepared.end_pts())
        .ok_or_else(|| TrackingError::Protocol("tracking range coverage is incomplete".into()))?;
    Ok(RangesResult {
        path,
        engine: engine.unwrap_or_default(),
        ranges: count,
        cached_ranges,
        cache_failures,
    })
}

/// The exclusive PTS end of a range's owned pictures.
fn range_end(plan: &RangePlan, range: PlannedRange, pictures: &[i64], end_pts: i64) -> i64 {
    if range.end == plan.pictures() {
        end_pts
    } else {
        pictures[range.end]
    }
}

/// Completed ranges as merged half-open PTS spans.
fn coverage(
    plan: &RangePlan,
    completed: &[bool],
    pictures: &[i64],
    end_pts: i64,
) -> Vec<(i64, i64)> {
    let mut spans: Vec<(usize, usize)> = plan
        .ranges()
        .iter()
        .zip(completed)
        .filter(|(_, done)| **done)
        .map(|(range, _)| (range.first, range.end))
        .collect();
    spans.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (first, end) in spans {
        match merged.last_mut() {
            Some(last) if last.1 == first => last.1 = end,
            _ => merged.push((first, end)),
        }
    }
    merged
        .into_iter()
        .map(|(first, end)| {
            let end_pts = if end == plan.pictures() {
                end_pts
            } else {
                pictures[end]
            };
            (pictures[first], end_pts)
        })
        .collect()
}
