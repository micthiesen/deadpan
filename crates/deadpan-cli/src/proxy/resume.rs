//! Resumable proxy encoding by completed ranges (specification §16.4).
//!
//! The Original is encoded in contiguous keyframe-aligned ranges of pictures
//! ([`proxy_segments`]), one isolated worker run per range. Each finished
//! range is appended to the key's private partial state, synchronized, hashed
//! and only then recorded in its journal, which is replaced atomically. A
//! cancelled, paused-then-closed or killed build therefore keeps every
//! recorded range; an interrupted range is simply encoded again. A later
//! build validates the journal against the current Original, recipe and plan,
//! rehashes every recorded range and encodes only what is missing. The
//! complete ranges are then joined at packet level by the worker into one
//! staged movie, which the caller verifies and publishes as before. Nothing
//! partial is ever served.

use std::fs::File;
use std::os::unix::fs::FileExt;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use deadpan_media::proxy::{
    PROXY_ENCODER, PROXY_PROTOCOL_VERSION, PROXY_RECIPE_VERSION, ProxyOriginal, ProxyPlan,
    ProxySegment, proxy_assemble_request, proxy_range_request, proxy_segments,
};
use deadpan_media::source_input::VerifiedSourceInput;
use deadpan_media::{ProxyEncodeOptions, assemble_proxy, encode_proxy_retrying};
use serde::{Deserialize, Serialize};

use super::cache::{ProxyCache, ProxyPartial, ProxyStaging};
use super::{BuildControl, ProxyBuildError, ProxySubject, remaining, stage_output};

/// Target pictures per range. A range starts at the first keyframe at or
/// after this many pictures, so it decodes without preroll. Measured on a
/// 4K30 Original (2026-10-06), each range's worker run, decoder opening and
/// VideoToolbox session cost about 0.1 s, and 600 4K pictures take about
/// 8 s to encode: an interruption loses at most that much.
pub const PROXY_SEGMENT_PICTURES: u64 = 600;
/// Journal format version.
pub const PROXY_JOURNAL_SCHEMA: u32 = 1;

/// The journal of completed ranges, `ranges.json` in the partial state. It
/// names the exact Original bytes, recipe, worker protocol, encoder, plan
/// and range rule it was made for; any difference discards it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    schema: u32,
    recipe: u32,
    protocol: u32,
    encoder: String,
    original: ProxyOriginal,
    width: u32,
    height: u32,
    frames: u64,
    segment_pictures: u64,
    ranges: Vec<JournalRange>,
}

/// One completed range: Original pictures `[start, end)`, encoded as a movie
/// at `[offset, offset + length)` of the ranges file with these bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct JournalRange {
    start: u64,
    end: u64,
    offset: u64,
    length: u64,
    blake3: String,
    extradata_sha256: String,
}

/// Live progress of one build, readable from another thread.
#[derive(Debug, Default)]
pub struct ProxyProgress {
    ranges: AtomicU64,
    pictures: AtomicU64,
    completed_pictures: AtomicU64,
    reused_ranges: AtomicU64,
    reused_pictures: AtomicU64,
    encoded_ranges: AtomicU64,
    encoded_pictures: AtomicU64,
}

/// What one build reused and encoded. `completed_pictures` counts both.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProxyProgressSnapshot {
    pub ranges: u64,
    pub pictures: u64,
    pub completed_pictures: u64,
    pub reused_ranges: u64,
    pub reused_pictures: u64,
    pub encoded_ranges: u64,
    pub encoded_pictures: u64,
}

impl ProxyProgress {
    pub fn snapshot(&self) -> ProxyProgressSnapshot {
        let load = |value: &AtomicU64| value.load(Ordering::Acquire);
        ProxyProgressSnapshot {
            ranges: load(&self.ranges),
            pictures: load(&self.pictures),
            completed_pictures: load(&self.completed_pictures),
            reused_ranges: load(&self.reused_ranges),
            reused_pictures: load(&self.reused_pictures),
            encoded_ranges: load(&self.encoded_ranges),
            encoded_pictures: load(&self.encoded_pictures),
        }
    }

    /// Completed share of the pictures, 0 to 100.
    pub fn percent(&self) -> u64 {
        let snapshot = self.snapshot();
        (snapshot.completed_pictures * 100)
            .checked_div(snapshot.pictures)
            .unwrap_or(0)
            .min(100)
    }
}

fn record(control: &BuildControl<'_>, update: impl FnOnce(&ProxyProgress)) {
    if let Some(progress) = control.progress {
        update(progress);
    }
}

fn lower_hex(text: &str, length: usize) -> bool {
    text.len() == length
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// BLAKE3 of `length` bytes at `offset`, checking cancellation per chunk.
fn hash_range(
    file: &File,
    offset: u64,
    length: u64,
    control: &BuildControl<'_>,
) -> Result<String, ProxyBuildError> {
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    let mut done = 0_u64;
    while done < length {
        if control.cancelled.load(Ordering::Acquire) {
            return Err(deadpan_media::ConversionError::Cancelled.into());
        }
        let want = usize::try_from((length - done).min(buffer.len() as u64)).unwrap_or(0);
        let count = file.read_at(&mut buffer[..want], offset + done)?;
        if count == 0 {
            return Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof).into());
        }
        hasher.update(&buffer[..count]);
        done += count as u64;
    }
    Ok(hasher.finalize().to_hex().to_string())
}

/// The journal a build for `subject` and `plan` starts from, and the ranges
/// it plans.
struct Resume {
    journal: Journal,
    planned: Vec<(u64, u64)>,
}

impl Resume {
    fn fresh(subject: &ProxySubject<'_>, plan: &ProxyPlan, target: u64) -> Self {
        let index = subject.index.index();
        Self {
            journal: Journal {
                schema: PROXY_JOURNAL_SCHEMA,
                recipe: PROXY_RECIPE_VERSION,
                protocol: PROXY_PROTOCOL_VERSION,
                encoder: PROXY_ENCODER.to_owned(),
                original: subject.identity.clone(),
                width: plan.width,
                height: plan.height,
                frames: index.frames().len() as u64,
                segment_pictures: target,
                ranges: Vec::new(),
            },
            planned: proxy_segments(index, target),
        }
    }

    /// Keep the recorded ranges of `bytes` that belong to this build and
    /// whose stored bytes still hash as recorded; anything else is dropped
    /// and encoded again. A journal for other bytes, another recipe, plan or
    /// range rule, or one that does not parse, is discarded whole.
    fn load(
        &mut self,
        bytes: Option<Vec<u8>>,
        data: &File,
        control: &BuildControl<'_>,
    ) -> Result<(), ProxyBuildError> {
        let Some(recorded) = bytes.and_then(|bytes| serde_json::from_slice::<Journal>(&bytes).ok())
        else {
            return Ok(());
        };
        let current = Journal {
            ranges: Vec::new(),
            ..recorded.clone()
        };
        if current
            != (Journal {
                ranges: Vec::new(),
                ..self.journal.clone()
            })
        {
            return Ok(());
        }
        let length = data.metadata()?.len();
        let mut kept: Vec<JournalRange> = Vec::new();
        for range in recorded.ranges {
            let planned = self.planned.contains(&(range.start, range.end));
            let Some(end) = range
                .offset
                .checked_add(range.length)
                .filter(|end| *end <= length)
            else {
                continue;
            };
            // Kept ranges lie inside the file, so their ends cannot overflow.
            let unique = kept.iter().all(|other| {
                other.start != range.start
                    && (range.offset >= other.offset + other.length || other.offset >= end)
            });
            if !planned
                || !unique
                || range.length == 0
                || !lower_hex(&range.blake3, 64)
                || !lower_hex(&range.extradata_sha256, 64)
            {
                continue;
            }
            if hash_range(data, range.offset, range.length, control)? == range.blake3 {
                kept.push(range);
            }
        }
        self.journal.ranges = kept;
        Ok(())
    }

    /// End of the last recorded range in the ranges file.
    fn data_end(&self) -> u64 {
        self.journal
            .ranges
            .iter()
            .map(|range| range.offset + range.length)
            .max()
            .unwrap_or(0)
    }

    fn missing(&self) -> Vec<(u64, u64)> {
        self.planned
            .iter()
            .copied()
            .filter(|(start, _)| {
                self.journal
                    .ranges
                    .iter()
                    .all(|range| range.start != *start)
            })
            .collect()
    }

    fn save(&self, partial: &ProxyPartial) -> Result<(), ProxyBuildError> {
        let bytes = serde_json::to_vec(&self.journal)
            .map_err(|error| ProxyBuildError::Ranges(error.to_string()))?;
        partial.write_journal(&bytes)?;
        Ok(())
    }

    /// The decoder configuration most ranges share (ties: the earliest
    /// range's), when they do not all share one.
    fn majority_configuration(&self) -> Option<String> {
        let mut counts: Vec<(String, usize, u64)> = Vec::new();
        for range in &self.journal.ranges {
            match counts
                .iter_mut()
                .find(|(digest, _, _)| *digest == range.extradata_sha256)
            {
                Some((_, count, first)) => {
                    *count += 1;
                    *first = (*first).min(range.start);
                }
                None => counts.push((range.extradata_sha256.clone(), 1, range.start)),
            }
        }
        if counts.len() <= 1 {
            return None;
        }
        counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.2.cmp(&b.2)));
        Some(counts.swap_remove(0).0)
    }
}

/// Encode every missing range of `subject`'s proxy into `partial`, then join
/// all ranges into a fresh staged movie. The staged movie is unverified.
#[allow(clippy::too_many_arguments)]
pub(super) fn encode_by_ranges(
    cache: &ProxyCache,
    partial: &ProxyPartial,
    subject: &ProxySubject<'_>,
    worker: &Path,
    plan: &ProxyPlan,
    input: &VerifiedSourceInput,
    control: &BuildControl<'_>,
    started: Instant,
) -> Result<ProxyStaging, ProxyBuildError> {
    let index = subject.index.index();
    let mut resume = Resume::fresh(subject, plan, control.segment_pictures);
    resume.load(partial.journal(), partial.data(), control)?;
    // Drop an interrupted range's tail and record what survived.
    partial.truncate(resume.data_end())?;
    resume.save(partial)?;
    let reused: u64 = resume
        .journal
        .ranges
        .iter()
        .map(|range| range.end - range.start)
        .sum();
    record(control, |progress| {
        progress
            .ranges
            .store(resume.planned.len() as u64, Ordering::Release);
        progress
            .pictures
            .store(index.frames().len() as u64, Ordering::Release);
        progress.completed_pictures.store(reused, Ordering::Release);
        progress
            .reused_ranges
            .store(resume.journal.ranges.len() as u64, Ordering::Release);
        progress.reused_pictures.store(reused, Ordering::Release);
    });
    let options = ProxyEncodeOptions {
        stall: control.stall,
        pause: control.pause,
    };
    // Ranges must share one decoder configuration to be joined. A minority
    // is encoded again once; if they still differ, everything is encoded
    // afresh once more, then the build fails.
    for round in 0.. {
        for (start, end) in resume.missing() {
            control.wait_while_paused()?;
            let offset = resume.data_end();
            let request = proxy_range_request(
                input.identity().byte_length(),
                subject.info,
                index,
                plan,
                start,
                end,
                offset,
                remaining(started, control)?,
            )?;
            let data = partial.data();
            let (_, report) = encode_proxy_retrying(
                worker,
                input,
                &request,
                || {
                    // A failed attempt's bytes are discarded before a retry.
                    data.set_len(offset)?;
                    Ok(((), data.try_clone()?))
                },
                control.cancelled,
                options,
            )?;
            data.sync_all()?;
            let blake3 = hash_range(data, offset, report.output_bytes, control)?;
            resume.journal.ranges.push(JournalRange {
                start,
                end,
                offset,
                length: report.output_bytes,
                blake3,
                extradata_sha256: report.extradata_sha256,
            });
            resume.save(partial)?;
            record(control, |progress| {
                progress.encoded_ranges.fetch_add(1, Ordering::AcqRel);
                progress
                    .encoded_pictures
                    .fetch_add(end - start, Ordering::AcqRel);
                progress
                    .completed_pictures
                    .fetch_add(end - start, Ordering::AcqRel);
            });
        }
        let Some(majority) = resume.majority_configuration() else {
            break;
        };
        match round {
            0 => resume
                .journal
                .ranges
                .retain(|range| range.extradata_sha256 == majority),
            1 => resume.journal.ranges.clear(),
            _ => {
                return Err(ProxyBuildError::Ranges(
                    "encoded ranges keep different decoder configurations".into(),
                ));
            }
        }
        partial.truncate(resume.data_end())?;
        resume.save(partial)?;
        let kept: u64 = resume
            .journal
            .ranges
            .iter()
            .map(|range| range.end - range.start)
            .sum();
        record(control, |progress| {
            progress.completed_pictures.store(kept, Ordering::Release);
        });
    }
    let mut ranges = resume.journal.ranges.clone();
    ranges.sort_by_key(|range| range.start);
    let frames = index.frames();
    let pts = |ordinal: u64| {
        usize::try_from(ordinal)
            .ok()
            .and_then(|ordinal| frames.get(ordinal))
            .map_or(index.terminal_end(), |frame| frame.pts)
    };
    let segments = ranges
        .iter()
        .map(|range| ProxySegment {
            offset: range.offset,
            length: range.length,
            frames: range.end - range.start,
            start_pts: pts(range.start),
            end_pts: pts(range.end),
        })
        .collect();
    control.wait_while_paused()?;
    let request = proxy_assemble_request(
        partial.data().metadata()?.len(),
        subject.info,
        plan,
        segments,
        remaining(started, control)?,
    )?;
    let (staging, output) = stage_output(cache)?;
    assemble_proxy(
        worker,
        partial.data(),
        &request,
        &output,
        control.cancelled,
        options,
    )?;
    Ok(staging)
}
