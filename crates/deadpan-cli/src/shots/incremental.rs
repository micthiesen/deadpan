//! Cursor-priority exact-picture scans. Every range includes real comparison
//! context; only complete global coverage becomes a stored ShotAnalysis.

use std::ops::Range;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use deadpan_analysis::incremental::{MAX_ANALYSIS_RANGES, RangeSchedule};
use deadpan_analysis::{
    MAX_SHOT_PICTURES, PictureMeasure, PictureSignature, REPLAY_PICTURES, SHOT_RULE,
    SIGNATURE_VERSION, ShotAnalysis, ShotMeasurer, decode_measures, encode_measures,
};
use deadpan_media::picture_scan::{PictureScanOptions, scan_pictures_range};
use deadpan_source::{DecodeLimits, DecodedRgbaFrame};
use serde::{Deserialize, Serialize};

use crate::transcription::incremental::cache::RangeCache;

use super::{SHOT_DECODE_THREADS, ShotInput, ShotScanError};

pub const SHOT_RANGE_IMPLEMENTATION: &str = "deadpan-shot-ranges-1";
pub const SHOT_RANGE_PICTURES: usize = 2_048;

#[derive(Debug, Clone)]
pub struct ShotRangeOptions {
    pub cache_directory: Option<PathBuf>,
    pub decode_threads: u32,
    /// Smaller windows are useful for qualification. Production uses 2048;
    /// very long sources enlarge a request to retain bounded bookkeeping.
    pub range_pictures: usize,
}

impl Default for ShotRangeOptions {
    fn default() -> Self {
        Self {
            cache_directory: None,
            decode_threads: SHOT_DECODE_THREADS,
            range_pictures: SHOT_RANGE_PICTURES,
        }
    }
}

pub struct ShotRangeProgress {
    pub coverage: Vec<Range<usize>>,
    pub completed_pictures: usize,
    pub total_pictures: usize,
    pub cached_ranges: usize,
    pub cache_failures: usize,
}

pub struct ShotRangeScan {
    pub key: deadpan_store::ShotAnalysisKey,
    pub analysis: ShotAnalysis,
    pub elapsed: Duration,
    pub signature_elapsed: Duration,
    /// Decoded context pictures, including overlapping halos; cached ranges
    /// contribute no decode work.
    pub decoded: usize,
    pub cached_ranges: usize,
    pub cache_failures: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Key {
    implementation: String,
    rule: String,
    source: deadpan_store::ShotAnalysisKey,
    pictures: usize,
    owned: Range<usize>,
    context: Range<usize>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CachedRange {
    key: Key,
    /// Full context retains the standalone canonical span validation. Only
    /// the disjoint owned measures are admitted to the global result.
    measures: Vec<u8>,
}

fn failure(error: impl std::fmt::Display) -> ShotScanError {
    ShotScanError::Failed(error.to_string())
}

fn validate_bounds(pictures: usize, options: &ShotRangeOptions) -> Result<(), ShotScanError> {
    // A qualified source index can exceed the analysis limit. Every local
    // context could still fit, so reject an impossible final analysis before
    // scheduling, decoding, caching, or allocating its complete measures.
    if !(1..=MAX_SHOT_PICTURES).contains(&pictures) {
        return Err(failure("shot picture count bound"));
    }
    if !(1..=8_192).contains(&options.range_pictures) || !(1..=16).contains(&options.decode_threads)
    {
        return Err(failure("shot range or decode-thread bound"));
    }
    Ok(())
}

fn check(cancelled: &AtomicBool, deadline: Instant) -> Result<(), ShotScanError> {
    if cancelled.load(Ordering::Acquire) {
        Err(ShotScanError::Cancelled)
    } else if Instant::now() >= deadline {
        Err(ShotScanError::Deadline)
    } else {
        Ok(())
    }
}

fn ordinals(range: &Range<u64>) -> Result<Range<usize>, ShotScanError> {
    Ok(usize::try_from(range.start).map_err(failure)?
        ..usize::try_from(range.end).map_err(failure)?)
}

fn owned_measures(key: &Key, measured: ShotAnalysis) -> Result<Vec<PictureMeasure>, ShotScanError> {
    if key.owned.start >= key.owned.end
        || key.context.start > key.owned.start
        || key.context.end < key.owned.end
        || key.context.end > key.pictures
        || measured.pictures() != key.context.end - key.context.start
        || key.owned.start.saturating_sub(REPLAY_PICTURES) != key.context.start
        || key
            .owned
            .end
            .saturating_add(REPLAY_PICTURES)
            .min(key.pictures)
            != key.context.end
    {
        return Err(failure("shot range context or coverage differs"));
    }
    let start = key.owned.start - key.context.start;
    let end = key.owned.end - key.context.start;
    measured
        .measures()
        .get(start..end)
        .map(|measures| measures.to_vec())
        .ok_or_else(|| failure("shot range has no owned measures"))
}

impl CachedRange {
    fn restore(&self, expected: &Key) -> Result<Vec<PictureMeasure>, ShotScanError> {
        if self.key != *expected {
            return Err(failure("shot range cache identity differs"));
        }
        let measures = decode_measures(&self.measures).map_err(failure)?;
        owned_measures(expected, ShotAnalysis::new(measures).map_err(failure)?)
    }
}

/// Decode one context range with the same bounded decode/signature overlap as
/// the sequential scanner. Pictures retain qualified global ordinals and PTS;
/// only the pure measurer receives local context ordinals.
fn measure_range(
    input: &ShotInput,
    key: &Key,
    decode_threads: u32,
    cancelled: &AtomicBool,
    deadline: Instant,
    boundary: &mut impl FnMut() -> Result<(), ShotScanError>,
) -> Result<(ShotAnalysis, Duration, usize), ShotScanError> {
    let video = input
        .receipt
        .snapshot()
        .video()
        .ok_or_else(|| failure("the Original has no qualified picture"))?;
    let mut measurer = ShotMeasurer::new(key.context.end - key.context.start).map_err(failure)?;
    let mut signature_elapsed = Duration::ZERO;
    let mut decoded = 0;
    std::thread::scope(|scope| -> Result<(), ShotScanError> {
        let (pictures, received) = mpsc::sync_channel::<(usize, DecodedRgbaFrame)>(2);
        let (reduced, signatures) = mpsc::channel();
        std::thread::Builder::new()
            .name("deadpan-shot-range-signatures".into())
            .spawn_scoped(scope, move || {
                for (ordinal, picture) in received {
                    let began = Instant::now();
                    let signature = PictureSignature::from_rgba(
                        &picture.rgba,
                        picture.width,
                        picture.height,
                        picture.row_stride_bytes,
                    );
                    let failed = signature.is_err();
                    if reduced
                        .send(signature.map(|signature| (ordinal, signature, began.elapsed())))
                        .is_err()
                        || failed
                    {
                        break;
                    }
                }
            })
            .map_err(failure)?;
        let mut absorb = |result: Result<(usize, PictureSignature, Duration), _>| {
            let (ordinal, signature, elapsed) = result.map_err(failure)?;
            let local = ordinal
                .checked_sub(key.context.start)
                .ok_or_else(|| failure("shot context ordinal"))?;
            measurer.push(local, signature).map_err(failure)?;
            signature_elapsed += elapsed;
            decoded += 1;
            Ok::<_, ShotScanError>(())
        };
        scan_pictures_range(
            &input.input,
            video,
            PictureScanOptions {
                decode: DecodeLimits {
                    threads: decode_threads,
                    ..DecodeLimits::default()
                },
                interpretation: input.key.interpretation.map(Into::into).unwrap_or_default(),
            },
            deadline,
            cancelled,
            key.context.clone(),
            |ordinal, picture| {
                boundary().map_err(|error| error.to_string())?;
                check(cancelled, deadline).map_err(|error| error.to_string())?;
                let picture = deadpan_media::analysis_picture::visible_picture(
                    picture,
                    video.interpretation(),
                    || check(cancelled, deadline).map_err(|error| error.to_string()),
                )?;
                pictures
                    .send((ordinal, picture))
                    .map_err(|_| "the shot signature thread stopped".to_owned())?;
                while let Ok(result) = signatures.try_recv() {
                    absorb(result).map_err(|error| error.to_string())?;
                }
                Ok::<_, String>(())
            },
        )
        .map_err(|error| {
            check(cancelled, deadline)
                .err()
                .unwrap_or_else(|| error.into())
        })?;
        drop(pictures);
        for result in signatures {
            absorb(result)?;
        }
        Ok(())
    })?;
    check(cancelled, deadline)?;
    Ok((
        measurer.finish().map_err(failure)?,
        signature_elapsed,
        decoded,
    ))
}

/// Run actual exact-picture analysis around the newest visible Original
/// ordinal. Cancellation retains completed cached ranges but never produces a
/// complete analysis. Changing the source interpretation creates a new key.
pub fn scan_shot_ranges(
    input: &ShotInput,
    cancelled: &AtomicBool,
    deadline: Instant,
    options: ShotRangeOptions,
    cursor: &AtomicU64,
    mut boundary: impl FnMut() -> Result<(), ShotScanError>,
    mut progress: impl FnMut(ShotRangeProgress),
) -> Result<ShotRangeScan, ShotScanError> {
    check(cancelled, deadline)?;
    let began = Instant::now();
    let total = input.pictures();
    validate_bounds(total, &options)?;
    // A public mutable label is not source authority, including on cache
    // hits where no decoder would otherwise check the retained input.
    let video = input.receipt.snapshot().video().ok_or_else(|| {
        ShotScanError::Unavailable("the Original has no qualified picture".into())
    })?;
    if input.key.content != input.receipt.original().content().to_string()
        || input.key.video_stream != video.index().stream_index()
        || input.key.signature_version != SIGNATURE_VERSION
        || input.input.identity() != video.index().content()
    {
        return Err(failure(
            "shot range key or input differs from the qualified Original",
        ));
    }
    video
        .interpretation_support()
        .validate(input.key.interpretation.map(Into::into).unwrap_or_default())
        .map_err(failure)?;
    let width = options
        .range_pictures
        .max(total.div_ceil(MAX_ANALYSIS_RANGES as usize));
    let mut schedule =
        RangeSchedule::new(total as u64, width as u64, REPLAY_PICTURES as u64).map_err(failure)?;
    let mut pieces: Vec<Option<Vec<PictureMeasure>>> = vec![None; total.div_ceil(width)];
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
    let mut cached_ranges = 0;
    let mut decoded = 0;
    let mut signature_elapsed = Duration::ZERO;
    let mut coverage = Vec::new();
    while let Some(range) = schedule
        .next(cursor.load(Ordering::Acquire))
        .map_err(failure)?
    {
        boundary()?;
        check(cancelled, deadline)?;
        let key = Key {
            implementation: SHOT_RANGE_IMPLEMENTATION.into(),
            rule: SHOT_RULE.into(),
            source: input.key.clone(),
            pictures: total,
            owned: ordinals(&range.owned)?,
            context: ordinals(&range.context)?,
        };
        let name = RangeCache::name(&key).map_err(failure)?;
        let cached = cache
            .as_ref()
            .and_then(|cache| match cache.read::<CachedRange>(&name) {
                Ok(Some(record)) => match record.restore(&key) {
                    Ok(measures) => Some(measures),
                    Err(_) => {
                        cache_failures += 1;
                        None
                    }
                },
                Ok(None) => None,
                Err(_) => {
                    cache_failures += 1;
                    None
                }
            });
        let measures = if let Some(measures) = cached {
            cached_ranges += 1;
            measures
        } else {
            let (analysis, elapsed, count) = measure_range(
                input,
                &key,
                options.decode_threads,
                cancelled,
                deadline,
                &mut boundary,
            )?;
            signature_elapsed += elapsed;
            decoded += count;
            let record = CachedRange {
                key: key.clone(),
                measures: encode_measures(analysis.measures()),
            };
            let measures = record.restore(&key)?;
            check(cancelled, deadline)?;
            if let Some(cache) = &cache
                && cache.write(&name, &record).is_err()
            {
                cache_failures += 1;
            }
            measures
        };
        check(cancelled, deadline)?;
        pieces[key.owned.start / width] = Some(measures);
        coverage.push(key.owned);
        coverage.sort_by_key(|range| range.start);
        schedule.finish(&range).map_err(failure)?;
        progress(ShotRangeProgress {
            coverage: coverage.clone(),
            completed_pictures: schedule.completed_samples() as usize,
            total_pictures: total,
            cached_ranges,
            cache_failures,
        });
    }
    check(cancelled, deadline)?;
    let mut measures = Vec::new();
    measures.try_reserve_exact(total).map_err(failure)?;
    for piece in pieces {
        measures.extend(piece.ok_or_else(|| failure("shot range coverage is incomplete"))?);
    }
    if measures.len() != total || !schedule.is_complete() {
        return Err(failure("shot range coverage is incomplete"));
    }
    Ok(ShotRangeScan {
        key: input.key.clone(),
        analysis: ShotAnalysis::new(measures).map_err(failure)?,
        elapsed: began.elapsed(),
        signature_elapsed,
        decoded,
        cached_ranges,
        cache_failures,
    })
}

#[cfg(test)]
mod tests;
