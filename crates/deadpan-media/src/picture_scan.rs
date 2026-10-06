//! Decode every picture of a verified source once, in presentation order.
//!
//! Analyses that measure all pictures (shot detection) open a fresh native
//! decoder over the private verified snapshot and visit each converted RGBA
//! picture with its qualified index ordinal. Every decoded picture must carry
//! exactly the PTS and duration its ordinal has in the qualified index, and
//! the stream must end exactly after the last indexed picture; a mismatch,
//! gap or extra picture fails explicitly instead of being guessed around.
//! Pictures are not retained: the caller keeps whatever it reduces them to.
//! Call this on a media worker thread, never a UI or audio callback.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use deadpan_source::{
    DecodeControl, DecodeLimits, DecodedRgbaFrame, SourceDecodeError, SourceDecoder,
};

use crate::source_input::VerifiedSourceInput;
use crate::source_qualification::QualifiedVideoSnapshot;

/// Pictures decoded before the target after a seek, at most. Matches the
/// preview session's seek bound.
pub const MAX_PREROLL_PICTURES: usize = 10_000;

#[derive(Debug, thiserror::Error)]
pub enum PictureScanError {
    #[error("picture scan cancelled")]
    Cancelled,
    #[error("picture scan deadline elapsed")]
    Deadline,
    #[error("decoded pictures differ from the qualified index: {0}")]
    Mismatch(String),
    #[error("picture visitor failed: {0}")]
    Visit(String),
    #[error(transparent)]
    Native(#[from] SourceDecodeError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Decode all pictures of `input` against its qualified `video` snapshot,
/// calling `visit(ordinal, picture)` for each in order. Returns the number of
/// pictures visited, which always equals the qualified index length.
pub fn scan_pictures<E: std::fmt::Display>(
    input: &VerifiedSourceInput,
    video: &QualifiedVideoSnapshot,
    limits: DecodeLimits,
    deadline: Instant,
    cancelled: &AtomicBool,
    mut visit: impl FnMut(usize, &DecodedRgbaFrame) -> Result<(), E>,
) -> Result<usize, PictureScanError> {
    scan_pictures_from(
        input,
        video,
        limits,
        deadline,
        cancelled,
        0,
        |ordinal, picture| visit(ordinal, &picture),
    )
}

/// Like [`scan_pictures`], starting at index ordinal `start` and handing
/// each picture over by value. A nonzero `start` seeks: the decoder restarts
/// at the key picture the qualified index names for `start` (its
/// `seek_from`), decodes the preroll pictures without converting them,
/// checks every preroll picture against its own indexed PTS and duration,
/// and then delivers `start` and every later picture exactly as a scan from
/// the first picture would. Returns the number of pictures visited.
pub fn scan_pictures_from<E: std::fmt::Display>(
    input: &VerifiedSourceInput,
    video: &QualifiedVideoSnapshot,
    limits: DecodeLimits,
    deadline: Instant,
    cancelled: &AtomicBool,
    start: usize,
    mut visit: impl FnMut(usize, DecodedRgbaFrame) -> Result<(), E>,
) -> Result<usize, PictureScanError> {
    let control = || -> Result<DecodeControl<'_>, PictureScanError> {
        if cancelled.load(Ordering::Acquire) {
            return Err(PictureScanError::Cancelled);
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(PictureScanError::Deadline);
        }
        Ok(DecodeControl {
            timeout: remaining.min(Duration::from_secs(60)),
            cancelled,
        })
    };
    // A native failure observed after cancellation or the deadline reports
    // that cause rather than the decoder's own interruption code.
    let native = |error: SourceDecodeError| -> PictureScanError {
        if cancelled.load(Ordering::Acquire) {
            PictureScanError::Cancelled
        } else if Instant::now() >= deadline {
            PictureScanError::Deadline
        } else {
            PictureScanError::Native(error)
        }
    };
    let index = video.index();
    if input.identity() != index.content() {
        return Err(PictureScanError::Mismatch(
            "the snapshot is not the qualified content".into(),
        ));
    }
    let mut decoder =
        SourceDecoder::open(input.decoder_file()?, limits, control()?).map_err(native)?;
    let info = decoder.info();
    let expected = video.interpretation();
    if info.stream_index != index.stream_index()
        || info.stream_index != expected.stream_index
        || (info.width, info.height) != (expected.width, expected.height)
        || (info.time_base_num, info.time_base_den)
            != (expected.time_base_num, expected.time_base_den)
    {
        return Err(PictureScanError::Mismatch(
            "the decoder selected a different picture stream or interpretation".into(),
        ));
    }
    let frames = index.index().frames();
    let mut ordinal = start;
    if start > 0 {
        let target = frames.get(start).ok_or_else(|| {
            PictureScanError::Mismatch(format!(
                "scan start {start} is beyond the {} indexed pictures",
                frames.len()
            ))
        })?;
        let anchor = target.seek_from.map_or(Ok(0), |anchor| {
            usize::try_from(anchor.0)
                .map_err(|_| PictureScanError::Mismatch("seek anchor beyond the index".into()))
        })?;
        let anchor = frames
            .get(anchor)
            .filter(|_| anchor <= start)
            .ok_or_else(|| {
                PictureScanError::Mismatch(format!("picture {start} has no seek anchor before it"))
            })?;
        decoder
            .seek_to(anchor.pts, target.pts, control()?)
            .map_err(native)?;
        let mut preroll = 0_usize;
        loop {
            let decoded = decoder
                .next_metadata(control()?)
                .map_err(native)?
                .ok_or_else(|| {
                    PictureScanError::Mismatch(format!("decoding ended before picture {start}"))
                })?;
            if decoded.pts >= target.pts {
                if decoded.pts != target.pts
                    || decoded.reported_duration != target.reported_duration
                {
                    return Err(PictureScanError::Mismatch(format!(
                        "seeking to picture {start} decoded PTS {} (duration {:?}), indexed at PTS {} (duration {:?})",
                        decoded.pts,
                        decoded.reported_duration,
                        target.pts,
                        target.reported_duration
                    )));
                }
                break;
            }
            // Every preroll picture must be an indexed picture before the target.
            let indexed = frames
                .binary_search_by_key(&decoded.pts, |indexed| indexed.pts)
                .is_ok_and(|position| {
                    position < start
                        && frames[position].reported_duration == decoded.reported_duration
                });
            if !indexed {
                return Err(PictureScanError::Mismatch(format!(
                    "preroll for picture {start} decoded unindexed PTS {}",
                    decoded.pts
                )));
            }
            preroll += 1;
            if preroll > MAX_PREROLL_PICTURES {
                return Err(PictureScanError::Mismatch(format!(
                    "seeking to picture {start} decoded more than {MAX_PREROLL_PICTURES} preroll pictures"
                )));
            }
        }
        let picture = decoder.copy_current_rgba(control()?).map_err(native)?;
        visit(start, picture).map_err(|error| PictureScanError::Visit(error.to_string()))?;
        ordinal += 1;
    }
    while let Some(picture) = decoder.next_rgba(control()?).map_err(native)? {
        let Some(indexed) = frames.get(ordinal) else {
            return Err(PictureScanError::Mismatch(format!(
                "picture {ordinal} decoded beyond the {} indexed pictures",
                frames.len()
            )));
        };
        if picture.metadata.pts != indexed.pts
            || picture.metadata.reported_duration != indexed.reported_duration
        {
            return Err(PictureScanError::Mismatch(format!(
                "picture {ordinal} decoded at PTS {} (duration {:?}), indexed at PTS {} (duration {:?})",
                picture.metadata.pts,
                picture.metadata.reported_duration,
                indexed.pts,
                indexed.reported_duration
            )));
        }
        visit(ordinal, picture).map_err(|error| PictureScanError::Visit(error.to_string()))?;
        ordinal += 1;
    }
    if ordinal != frames.len() {
        return Err(PictureScanError::Mismatch(format!(
            "decoding ended after {ordinal} of {} indexed pictures",
            frames.len()
        )));
    }
    Ok(ordinal - start)
}
